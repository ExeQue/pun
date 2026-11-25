<?php

// pun's php-rules worker (prototype): runs a project's own PHPStan rule classes (`[analyse] php-rules`) on the files
// `pun analyse` checks. pun runs it with `php -r`, so no script lands in a temporary directory.
//
// It loads the project's vendor/autoload.php and PHPStan's phar (vendor/phpstan/phpstan/phpstan.phar), and boots
// PHPStan's own container from the php-rules neon files, without a level's config, so the registry holds only the
// rules those files register and the reflection provider is PHPStan's real one. Each file is parsed with PHPStan's
// rich parser (so `phpstan.parser.richParserNodeVisitor` services run) and walked; every node goes to the rules
// whose getNodeType() it is an instance of, with a Scope of which only a subset works (PunScopeBase below).
// `getType()` asks pun for the type of the expression's byte span, as a PHPDoc type string.
//
// Protocol: JSON lines. pun's first line is the setup:
//   {"token": ..., "root": ..., "autoload": ..., "phar": ..., "neon": [...], "paths": [...]}
// The worker answers each request on stdout with a line that starts with the token (not an environment variable:
// a containerized `php` such as lerd's doesn't pass the environment on); any other output is not the worker's and
// pun passes it on to stderr:
//   {"ready": <number of rules>} | {"error": "..."}
// Then per file, pun sends {"file": "/abs/path.php"}, plus "class" and "trait" for a trait's body in the context of a
// class that uses it; the worker may ask {"type": [start, end]} (byte offsets,
// end exclusive) any number of times, each answered by pun with {"type": "<PHPDoc type>" | null}, and finishes
// with {"errors": [{"message", "line", "identifier", "tip", "nonIgnorable"}]} or {"error": "..."}.

ini_set('display_errors', 'stderr');
error_reporting(E_ALL & ~E_DEPRECATED);
ob_start(); // rule output must not reach the protocol
define('__PHPSTAN_RUNNING__', true);

final class PunWorker
{
    public static string $token;

    /** @param array<mixed> $message */
    public static function send(array $message): void
    {
        fwrite(STDOUT, self::$token . json_encode($message, JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE) . "\n");
        fflush(STDOUT);
    }

    /** @return array<mixed> */
    public static function receive(): array
    {
        $line = fgets(STDIN);
        if ($line === false) {
            exit(0);
        }

        return json_decode($line, true, 512, JSON_THROW_ON_ERROR);
    }
}

function pun_describe(Throwable $e): string
{
    $at = $e instanceof PunUnsupported ? '' : ' (' . get_class($e) . ' at ' . $e->getFile() . ':' . $e->getLine() . ')';

    return $e->getMessage() . $at;
}

final class PunUnsupported extends LogicException
{
}

/** @param array<mixed> $setup */
function pun_boot(array $setup): PHPStan\DependencyInjection\Container
{
    $tmp = sys_get_temp_dir() . '/pun-php-rules-' . md5($setup['root']);
    @mkdir($tmp, 0700, true);
    $neon = $tmp . '/pun.neon';
    $lines = ['includes:'];
    foreach ($setup['neon'] as $file) {
        $lines[] = '    - ' . json_encode($file);
    }
    $lines[] = 'parameters:';
    $lines[] = '    paths:';
    foreach ($setup['paths'] as $path) {
        $lines[] = '        - ' . json_encode($path);
    }
    file_put_contents($neon, implode("\n", $lines) . "\n");
    $factory = new PHPStan\DependencyInjection\ContainerFactory($setup['root']);

    return $factory->create($tmp, [$neon], $setup['paths'], [$setup['root']], $setup['paths'], '0');
}

/** The virtual nodes the worker builds. */
const PUN_VIRTUAL_NODES = [
    PHPStan\Node\InClassNode::class,
    PHPStan\Node\InClassMethodNode::class,
    PHPStan\Node\FileNode::class,
];

/** @return list<PHPStan\Rules\Rule<PhpParser\Node>> */
function pun_rules(PHPStan\DependencyInjection\Container $container): array
{
    // ponytail: PHPStan's config.neon registers some of its own rules at every level; Pun runs PHPStan's rules
    // natively, so the worker keeps the rest. Reading the php-rules neon files' `rules:` would be exact.
    $rules = array_filter(
        $container->getServicesByTag('phpstan.rules.rule'),
        static fn (object $rule): bool => !str_starts_with(get_class($rule), 'PHPStan\\'),
    );
    foreach ($rules as $rule) {
        $type = $rule->getNodeType();
        if (is_subclass_of($type, PHPStan\Node\VirtualNode::class) && !in_array($type, PUN_VIRTUAL_NODES, true)) {
            throw new PunUnsupported(get_class($rule) . " asks for {$type}, which the php-rules prototype does not build (it builds " . implode(', ', PUN_VIRTUAL_NODES) . ')');
        }
    }

    return array_values($rules);
}

/**
 * @param list<PHPStan\Rules\Rule<PhpParser\Node>> $rules
 * @return list<array<string, mixed>>
 */
function pun_check(PHPStan\DependencyInjection\Container $container, array $rules, string $file, ?string $class = null, ?string $trait = null): array
{
    /** @var PHPStan\Parser\Parser $parser */
    $parser = $container->getService('currentPhpVersionRichParser');
    $walker = new PunWalker(
        $rules,
        $container->getByType(PHPStan\Reflection\ReflectionProvider::class),
        $container->getByType(PHPStan\PhpDoc\TypeStringResolver::class),
        $file,
    );
    $stmts = $parser->parseFile($file);
    if ($trait === null) {
        $walker->visit(new PHPStan\Node\FileNode($stmts));
        $walker->walk($stmts);
    } else {
        $walker->inTrait($stmts, $walker->reflection->getClass($class), $walker->reflection->getClass($trait));
    }

    return $walker->errors;
}

final class PunWalker
{
    /** @var list<array<string, mixed>> */
    public array $errors = [];

    /** @var array<class-string, list<PHPStan\Rules\Rule<PhpParser\Node>>> */
    private array $byType = [];

    public ?string $namespace = null;

    public ?PHPStan\Reflection\ClassReflection $class = null;

    public ?string $function = null;

    public ?PHPStan\Reflection\ClassReflection $trait = null;

    /** @var array<string, PHPStan\Type\Type> */
    private array $types = [];

    /** @param list<PHPStan\Rules\Rule<PhpParser\Node>> $rules */
    public function __construct(
        private array $rules,
        public PHPStan\Reflection\ReflectionProvider $reflection,
        private PHPStan\PhpDoc\TypeStringResolver $typeStrings,
        public string $file,
    ) {
    }

    /** @param array<PhpParser\Node|mixed> $nodes */
    public function walk(array $nodes): void
    {
        foreach ($nodes as $node) {
            if ($node instanceof PhpParser\Node) {
                $this->node($node);
            } elseif (is_array($node)) {
                $this->walk($node);
            }
        }
    }

    /**
     * Walks the body of `$trait` (declared in `$nodes`) in the context of `$class`, which uses it, as PHPStan does.
     *
     * @param array<PhpParser\Node> $nodes
     */
    public function inTrait(array $nodes, PHPStan\Reflection\ClassReflection $class, PHPStan\Reflection\ClassReflection $trait): void
    {
        foreach ($nodes as $node) {
            if ($node instanceof PhpParser\Node\Stmt\Namespace_) {
                $this->namespace = $node->name?->toString();
                $this->inTrait($node->stmts, $class, $trait);
                $this->namespace = null;
            } elseif ($node instanceof PhpParser\Node\Stmt\Trait_ && strcasecmp((string) $node->namespacedName, $trait->getName()) === 0) {
                [$this->class, $this->trait] = [$class, $trait];
                $this->walk($node->stmts);
                [$this->class, $this->trait] = [null, null];
            }
        }
    }

    private function node(PhpParser\Node $node): void
    {
        $this->visit($node);
        // PHPStan also hands a nullsafe call or fetch to the rules as its plain counterpart
        if ($node instanceof PhpParser\Node\Expr\NullsafeMethodCall) {
            $this->visit(new PhpParser\Node\Expr\MethodCall($node->var, $node->name, $node->args, $node->getAttributes()));
        } elseif ($node instanceof PhpParser\Node\Expr\NullsafePropertyFetch) {
            $this->visit(new PhpParser\Node\Expr\PropertyFetch($node->var, $node->name, $node->getAttributes()));
        }
        $saved =[$this->namespace, $this->class, $this->function];
        if ($node instanceof PhpParser\Node\Stmt\Namespace_) {
            $this->namespace = $node->name?->toString();
        } elseif ($node instanceof PhpParser\Node\Stmt\Trait_) {
            // PHPStan walks a trait's body only in the context of each class that uses it
            return;
        } elseif ($node instanceof PhpParser\Node\Stmt\ClassLike) {
            $name = $node->namespacedName?->toString();
            if ($name === null || !$this->reflection->hasClass($name)) {
                // ponytail: an anonymous class's body runs without a class reflection; PHPStan's
                // getAnonymousClassReflection() needs its scope
                $this->class = null;
            } else {
                $this->class = $this->reflection->getClass($name);
                $this->visit(new PHPStan\Node\InClassNode($node, $this->class));
            }
            $this->function = null;
        } elseif ($node instanceof PhpParser\Node\Stmt\ClassMethod) {
            $this->function = $node->name->toString();
            if ($this->class !== null) {
                $this->visit(pun_in_class_method($this->class, $node));
            }
        } elseif ($node instanceof PhpParser\Node\Stmt\Function_) {
            $this->function = $node->namespacedName?->toString() ?? $node->name->toString();
        }
        foreach ($node->getSubNodeNames() as $name) {
            $sub = $node->$name;
            if ($sub instanceof PhpParser\Node) {
                $this->node($sub);
            } elseif (is_array($sub)) {
                $this->walk($sub);
            }
        }
        [$this->namespace, $this->class, $this->function] = $saved;
    }

    public function visit(PhpParser\Node $node): void
    {
        $class = get_class($node);
        $rules = $this->byType[$class] ??= array_values(array_filter(
            $this->rules,
            static fn (PHPStan\Rules\Rule $rule): bool => $node instanceof ($rule->getNodeType()),
        ));
        if ($rules === []) {
            return;
        }
        $scope = new PunScope($this);
        foreach ($rules as $rule) {
            try {
                $found = $rule->processNode($node, $scope);
            } catch (PunUnsupported $e) {
                throw new PunUnsupported(get_class($rule) . ': ' . $e->getMessage(), 0, $e);
            } catch (Error $e) {
                // e.g. an InClassMethodNode's method reflection, which the prototype leaves unset
                throw new PunUnsupported(get_class($rule) . ' failed on ' . $this->file . ':' . $node->getStartLine() . ': ' . $e->getMessage(), 0, $e);
            }
            foreach ($found as $error) {
                $this->errors[] = $this->error($error, $node);
            }
        }
    }

    /** @return array<string, mixed> */
    private function error(mixed $error, PhpParser\Node $node): array
    {
        if (is_string($error)) {
            return ['message' => $error, 'line' => $node->getStartLine(), 'identifier' => null, 'tip' => null, 'nonIgnorable' => false];
        }
        if ($error instanceof PHPStan\Rules\FileRuleError && $error->getFile() !== $this->file) {
            throw new PunUnsupported('a rule reported an error in another file (' . $error->getFile() . '), which the php-rules prototype does not support');
        }

        return [
            'message' => $error->getMessage(),
            'line' => $error instanceof PHPStan\Rules\LineRuleError && $error->getLine() !== -1 ? $error->getLine() : $node->getStartLine(),
            'identifier' => $error instanceof PHPStan\Rules\IdentifierRuleError ? $error->getIdentifier() : null,
            'tip' => $error instanceof PHPStan\Rules\TipRuleError ? $error->getTip() : null,
            'nonIgnorable' => $error instanceof PHPStan\Rules\NonIgnorableRuleError,
        ];
    }

    public function type(PhpParser\Node\Expr $expr): PHPStan\Type\Type
    {
        $start = $expr->getStartFilePos();
        $end = $expr->getEndFilePos();
        if ($start < 0 || $end < 0) {
            // a node a rule built itself: pun has no type for it
            return new PHPStan\Type\MixedType();
        }
        $key = $start . ':' . $end;
        if (isset($this->types[$key])) {
            return $this->types[$key];
        }
        PunWorker::send(['type' => [$start, $end + 1]]);
        $answer = PunWorker::receive()['type'] ?? null;
        $type = $answer === null
            ? new PHPStan\Type\MixedType()
            : $this->typeStrings->resolve($answer, new PHPStan\Analyser\NameScope(null, [], $this->class?->getName(), $this->function));

        return $this->types[$key] = $type;
    }
}

/** An InClassMethodNode without its method reflection, which only PHPStan's NodeScopeResolver builds. */
function pun_in_class_method(PHPStan\Reflection\ClassReflection $class, PhpParser\Node\Stmt\ClassMethod $method): PHPStan\Node\InClassMethodNode
{
    $node = (new ReflectionClass(PHPStan\Node\InClassMethodNode::class))->newInstanceWithoutConstructor();
    (function () use ($class, $method): void {
        $this->classReflection = $class;
        $this->originalNode = $method;
        $this->attributes = $method->getAttributes();
    })->call($node);

    return $node;
}

/** The Scope methods the prototype answers; PunScope throws for the others. */
abstract class PunScopeBase
{
    public function __construct(protected PunWalker $walker)
    {
    }

    public function getFile(): string
    {
        return $this->walker->file;
    }

    public function getFileDescription(): string
    {
        return $this->walker->file;
    }

    public function getNamespace(): ?string
    {
        return $this->walker->namespace;
    }

    public function isInClass(): bool
    {
        return $this->walker->class !== null;
    }

    public function getClassReflection(): ?PHPStan\Reflection\ClassReflection
    {
        return $this->walker->class;
    }

    public function isInTrait(): bool
    {
        return $this->walker->trait !== null;
    }

    public function getTraitReflection(): ?PHPStan\Reflection\ClassReflection
    {
        return $this->walker->trait;
    }

    public function getFunctionName(): ?string
    {
        return $this->walker->function;
    }

    public function resolveName(PhpParser\Node\Name $name): string
    {
        $lower = $name->toLowerString();
        $class = $this->walker->class;
        if (in_array($lower, ['self', 'static'], true) && $class !== null) {
            return $class->getName();
        }
        if ($lower === 'parent' && $class?->getParentClass() !== null) {
            return $class->getParentClass()->getName();
        }

        return $name->toString();
    }

    public function getType(PhpParser\Node\Expr $node): PHPStan\Type\Type
    {
        return $this->walker->type($node);
    }
}

/** Defines PunScope: PunScopeBase plus a throwing body for every other method of PHPStan's Scope interface. */
function pun_define_scope(): void
{
    $type = static function (?ReflectionType $t) use (&$type): string {
        if ($t === null) {
            return '';
        }
        if ($t instanceof ReflectionUnionType || $t instanceof ReflectionIntersectionType) {
            $glue = $t instanceof ReflectionUnionType ? '|' : '&';

            return implode($glue, array_map(static fn ($p) => $p instanceof ReflectionIntersectionType ? '(' . $type($p) . ')' : $type($p), $t->getTypes()));
        }
        $name = $t->getName();
        $name = $t->isBuiltin() || in_array($name, ['self', 'static'], true) ? $name : '\\' . $name;

        return ($t->allowsNull() && !in_array($name, ['mixed', 'null'], true) ? '?' : '') . $name;
    };
    $base = new ReflectionClass(PunScopeBase::class);
    $methods = [];
    foreach ((new ReflectionClass(PHPStan\Analyser\Scope::class))->getMethods() as $method) {
        if ($base->hasMethod($method->getName())) {
            continue;
        }
        $params = [];
        foreach ($method->getParameters() as $p) {
            $code = $type($p->getType()) . ' ' . ($p->isPassedByReference() ? '&' : '') . ($p->isVariadic() ? '...' : '') . '$' . $p->getName();
            if ($p->isDefaultValueAvailable()) {
                $code .= ' = ' . ($p->isDefaultValueConstant() ? '\\' . $p->getDefaultValueConstantName() : var_export($p->getDefaultValue(), true));
            }
            $params[] = $code;
        }
        $return = $method->hasReturnType() ? ': ' . $type($method->getReturnType()) : '';
        $name = $method->getName();
        $methods[] = "public function {$name}(" . implode(', ', $params) . "){$return} { throw new \\PunUnsupported('Scope::{$name}() is not supported by the php-rules prototype'); }";
    }
    eval('final class PunScope extends PunScopeBase implements PHPStan\Analyser\Scope {' . implode("\n", $methods) . '}');
}

$setup = PunWorker::receive();
PunWorker::$token = $setup['token'];

try {
    foreach (['autoload' => 'the project\'s Composer autoloader', 'phar' => 'PHPStan\'s phar'] as $key => $what) {
        if (!is_file($setup[$key])) {
            throw new RuntimeException("{$setup[$key]} does not exist: php-rules needs {$what}");
        }
    }
    $pharLoader = require 'phar://' . $setup['phar'] . '/vendor/autoload.php';
    require_once 'phar://' . $setup['phar'] . '/preload.php';
    $pharLoader->unregister();
    require_once $setup['autoload'];
    // PHPStan's own classes (and its php-parser) win over the project's copies, as in bin/phpstan
    $pharLoader->register(true);
    pun_define_scope();
    $container = pun_boot($setup);
    $rules = pun_rules($container);
    PunWorker::send(['ready' => count($rules)]);
} catch (Throwable $e) {
    PunWorker::send(['error' => pun_describe($e)]);
    exit(1);
}

while (true) {
    $request = PunWorker::receive();
    try {
        $errors = pun_check($container, $rules, $request['file'], $request['class'] ?? null, $request['trait'] ?? null);
        PunWorker::send(['errors' => $errors]);
    } catch (Throwable $e) {
        PunWorker::send(['error' => pun_describe($e)]);
    }
    ob_clean();
}
