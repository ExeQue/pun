// `pun migrate` (run with `php -r`): a .php-cs-fixer(.dist).php's rules as PHP-CS-Fixer's own ConfigurationResolver
// resolves them (rule validation, custom fixers), and its Finder, printed as one JSON object:
// {"rules": {...}, "finder": {"in": [...], "exclude": [...], "notName": [...], "notPath": [...], "unsupported": [...]}}.
// The rules carry setIndent("\t") and setLineEnding() as options of indentation_type and line_ending.
// The Finder keys are pint.json's; `in` is relative to the project when inside it; `unsupported` names each Finder
// setting pun.toml has no key for. On failure, its message and exit 1. The JSON or the message follows the marker
// "\0pun-migrate\0", as output written straight to STDOUT (a deprecation notice) gets past the buffer below.
// argv: the project's vendor/autoload.php, the config path, and the PHP-CS-Fixer: the autoload.php of its vendor/,
// or its executable (a PHAR, or a bin whose autoloader is found as PHP-CS-Fixer's bin finds it).
error_reporting(E_ALL & ~E_DEPRECATED & ~E_USER_DEPRECATED);
ob_start(static fn () => '', 4096); // the config's own output must not reach stdout

[, $autoload, $config, $fixer] = $argv;
try {
    if (is_file($autoload)) {
        require_once $autoload;
    }
    // Required after the project's: Composer prepends its loader, so PHP-CS-Fixer's own dependencies win.
    if (basename($fixer) !== 'autoload.php') {
        $bin = realpath($fixer) ?: $fixer;
        try {
            Phar::loadPhar($bin, 'pun-php-cs-fixer.phar');
            $fixer = 'phar://pun-php-cs-fixer.phar/vendor/autoload.php';
        } catch (PharException|UnexpectedValueException) {
            $found = array_filter([dirname($bin).'/../../autoload.php', dirname($bin).'/../autoload.php', dirname($bin).'/vendor/autoload.php'], 'is_file');
            if ($found === []) {
                throw new RuntimeException("no autoload.php for {$fixer}");
            }
            $fixer = reset($found);
        }
    }
    require_once $fixer;
    $resolver = new PhpCsFixer\Console\ConfigurationResolver(new PhpCsFixer\Config(), ['config' => $config, 'path' => [], 'path-mode' => 'override', 'dry-run' => true], getcwd(), new PhpCsFixer\ToolInfo());
    $resolver->getFixers();
    $finder = $resolver->getConfig()->getFinder();
    $out = ['in' => [], 'exclude' => [], 'notName' => [], 'notPath' => [], 'unsupported' => []];
    if (!$finder instanceof Symfony\Component\Finder\Finder) {
        $out['unsupported'][] = 'a '.get_debug_type($finder).' as the Finder';
    } else {
        // An older symfony/finder (PHP-CS-Fixer's PHAR bundles 5.4) lacks some of these properties, so it has none set.
        $get = static fn (string $p) => property_exists(Symfony\Component\Finder\Finder::class, $p) ? (new ReflectionProperty(Symfony\Component\Finder\Finder::class, $p))->getValue($finder) : [];
        // Real paths on both sides: __DIR__ and getcwd() can differ by a symlink (macOS /var is /private/var).
        $root = rtrim(realpath(getcwd()), '/').'/';
        foreach ($get('dirs') as $dir) {
            $dir = rtrim(realpath($dir) ?: $dir, '/');
            if ($dir.'/' !== $root) {
                $out['in'][] = str_starts_with($dir, $root) ? substr($dir, strlen($root)) : $dir;
            }
        }
        // PhpCsFixer\Finder's own defaults: `*.php` files, `vendor` excluded (Pun's finder does both).
        $out['exclude'] = array_values(array_diff($get('exclude'), ['vendor']));
        $out['notName'] = array_values($get('notNames'));
        $out['notPath'] = array_values($get('notPaths'));
        foreach (array_diff($get('names'), ['/\.php$/', '*.php']) as $n) {
            $out['unsupported'][] = 'name('.json_encode($n, JSON_UNESCAPED_SLASHES).')';
        }
        foreach (['paths' => 'path', 'depths' => 'depth', 'contains' => 'contains', 'notContains' => 'notContains', 'sizes' => 'size', 'dates' => 'date', 'filters' => 'filter', 'pruneFilters' => 'filter', 'iterators' => 'append'] as $p => $call) {
            if ($get($p) !== [] && !in_array($call.'()', $out['unsupported'], true)) {
                $out['unsupported'][] = $call.'()';
            }
        }
        if ($get('followLinks')) {
            $out['unsupported'][] = 'followLinks()';
        }
        if ($get('ignore') & Symfony\Component\Finder\Finder::IGNORE_VCS_IGNORED_FILES) {
            $out['unsupported'][] = 'ignoreVCSIgnored()';
        }
    }
    // setIndent() and setLineEnding() are options of the rules that enforce them in pun.toml (2-space indent is not).
    $rules = $resolver->getRules();
    $whitespace = $resolver->getConfig();
    if ($whitespace->getIndent() === "\t") {
        $rules['indentation_type'] = ['indent' => "\t"];
    } elseif ($whitespace->getIndent() !== '    ') {
        $out['unsupported'][] = 'setIndent('.json_encode($whitespace->getIndent()).')';
    }
    if ($whitespace->getLineEnding() !== "\n") {
        $rules['line_ending'] = ['line_ending' => $whitespace->getLineEnding()];
    }
    fwrite(STDOUT, "\0pun-migrate\0".json_encode(['rules' => (object) $rules, 'finder' => $out], JSON_UNESCAPED_SLASHES));
} catch (Throwable $e) {
    fwrite(STDOUT, "\0pun-migrate\0".$e->getMessage());
    exit(1);
}
