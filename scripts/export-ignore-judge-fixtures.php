<?php

declare(strict_types=1);

/*
 * Exact results of PHPStan 2.3.6's UnmatchedIgnoredErrorJudge, from the phar's own classes:
 *
 *   crates/pun-phpstan/tests/fixtures/ignore-judge.json  IgnoredErrorHelper::initialize() + process() with the
 *       analysed paths, the configured paths, excludePaths and the collected data the judge reads (paths under
 *       crates/pun-phpstan/tests/fixtures/ignore/files, as {dir})
 *
 * The oracle (.oracle/phpstan) pins 2.3.5, which has no judge, so this takes a 2.3.6 phar:
 *
 *   composer require -d /tmp/phpstan-236 phpstan/phpstan:2.3.6
 *   php scripts/export-ignore-judge-fixtures.php /tmp/phpstan-236/vendor/phpstan/phpstan/phpstan.phar
 */

namespace PHPStan\Testing {
    abstract class PHPStanTestCase {}
}

namespace {
    use PHPStan\Analyser\AnalysedDeclarationDependencyCollector;
    use PHPStan\Analyser\AnalysedFileDependencyCollector;
    use PHPStan\Analyser\Error;
    use PHPStan\Analyser\Ignore\IgnoredErrorHelper;
    use PHPStan\Analyser\ResultCache\LazyCollectedData;
    use PHPStan\File\FileExcluder;
    use PHPStan\File\FileHelper;
    use PHPStan\Rules\Traits\TraitDeclarationCollector;

    $root = dirname(__DIR__);
    Phar::loadPhar($argv[1], 'phpstan.phar');
    require 'phar://phpstan.phar/vendor/autoload.php';

    function tagged(mixed $v): mixed
    {
        if (is_array($v)) {
            $items = [];
            foreach ($v as $k => $x) {
                $items[] = [$k, tagged($x)];
            }

            return ['array' => $items];
        }

        return $v;
    }

    $fixer = (new ReflectionClass(PHPStan\Analyser\Ignore\IgnoreErrorsConfigFixer::class))->newInstanceWithoutConstructor();
    foreach (['fileHelper' => new FileHelper($root), 'allConfigFiles' => [], 'rootDir' => $root, 'currentWorkingDirectory' => $root] as $k => $v) {
        (new ReflectionProperty($fixer, $k))->setValue($fixer, $v);
    }

    $fixtures = $root.'/crates/pun-phpstan/tests/fixtures';
    $dir = $fixtures.'/ignore/files';
    $a = $dir.'/a.php';
    $b = $dir.'/b.php';
    $c = $dir.'/sub/c.php';
    $t = $dir.'/trait.php';
    $other = $dir.'/other.php';
    $vendor = $root.'/vendor/lib.php';
    $e = static fn (string $message, string $file = 'a.php', int $line = 1): Error => new Error($message, $dir.'/'.$file, $line, true, $dir.'/'.$file);
    $files = AnalysedFileDependencyCollector::class;
    $declarations = AnalysedDeclarationDependencyCollector::class;
    $traits = TraitDeclarationCollector::class;
    $entries = [
        ['message' => '#^NeverA$#', 'path' => $a],
        ['message' => '#^NeverB$#', 'path' => $b],
        ['message' => '#^NeverC$#', 'path' => $c],
        ['message' => '#^NeverSub$#', 'path' => $dir.'/sub'],
        ['message' => '#^NeverPattern$#', 'path' => $dir.'/sub/*.php'],
        ['message' => '#^NeverWild$#', 'path' => '*.php'],
        ['message' => '#^NeverMissing$#', 'path' => $dir.'/sub/missing.php'],
        ['message' => '#^NeverPaths$#', 'paths' => [$a, $dir.'/nowhere/*', $c]],
        ['message' => '#^NeverNowhere$#', 'paths' => [$dir.'/nowhere/*']],
        ['message' => '#^Foo$#', 'path' => $a, 'count' => 2],
        ['message' => '#^NoPath$#'],
        '#^String$#',
    ];
    $errors = [$e('Foo'), $e('Bar', 'b.php', 2)];
    // [analysed files, analysed paths, configured paths, excludePaths, collected data (null: none), onlyFiles, rule skipped]
    $cases = [
        'whole project' => [[$a, $b, $c, $t], [$dir], [$dir], [], null, false, false],
        'configured files' => [[$a], [$a], [$a], [], [], true, false],
        'without collected data' => [[$a, $c], [$a, $c], [$dir], [], null, true, false],
        'no dependencies' => [[$a, $b, $c], [$a, $b, $dir.'/sub'], [$dir], [], [], false, false],
        'only files' => [[$a, $b, $c], [$a, $b, $c], [$dir], [], [], true, false],
        'rule skipped' => [[$a, $b, $c], [$a, $b, $c], [$dir], [], [], true, true],
        'trait declarations' => [[$a, $c], [$a, $c], [$dir], [], [$a => [$traits => [['T', 3]]], $c => [$traits => []]], true, false],
        'not analysed dependency' => [[$a, $b, $c], [$a, $b, $c], [$dir], [], [$a => [$files => [$other]], $c => [$files => [$t]]], true, false],
        'vendor dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$vendor]]], true, false],
        'excluded dependency' => [[$a, $b], [$a, $b], [$dir], [$other], [$a => [$files => [$other]]], true, false],
        'project dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [null]]], true, false],
        'without configured paths' => [[$a, $b], [$a, $b], [], [], [$a => [$files => [$vendor]]], true, false],
        'analysed dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$b, $a]], $b => [$files => [$a]]], true, false],
        'dependency of a dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$b]], $b => [$files => [$other]]], true, false],
        'declarations of a dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$b]], $b => [$declarations => [$other]]], true, false],
        'own declarations' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$declarations => [$other]]], true, false],
        'project dependency of a dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$b]], $b => [$files => [null]]], true, false],
        'dependency judged before' => [[$a, $b], [$a, $b], [$dir], [], [$b => [$files => [$other]], $a => [$files => [$b]]], true, false],
        'judged dependency' => [[$a, $b], [$a, $b], [$dir], [], [$a => [$files => [$b]], $b => [$declarations => [$other]], $c => [$files => [$b]]], true, false],
        'directory analysed' => [[$c], [$dir.'/sub'], [$dir], [], [], false, false],
        'sibling directory' => [[$a], [$dir.'/su'], [$dir], [], [], false, false],
    ];
    $out = [];
    foreach ($cases as $name => [$analysed, $paths, $configured, $excludes, $collected, $onlyFiles, $skipped]) {
        $helper = (new IgnoredErrorHelper(new FileHelper($root), $fixer, new FileExcluder(new FileHelper($root), $excludes), $entries, true, $paths, $configured))->initialize();
        $result = $helper->process($errors, $onlyFiles, $analysed, false, $skipped, $collected === null ? null : LazyCollectedData::fromArray($collected));
        $placeholder = static fn (mixed $v) => json_decode(str_replace([addcslashes($dir, '/'), addcslashes($root, '/')], ['{dir}', '{root}'], json_encode($v)), true);
        $data = [];
        foreach ($collected ?? [] as $file => $byCollector) {
            $data[$file] = [
                'traits' => $byCollector[$traits] ?? [],
                'files' => $byCollector[$files] ?? [],
                'declarations' => $byCollector[$declarations] ?? [],
            ];
        }
        $out[] = $placeholder([
            'name' => $name,
            'analysedFiles' => $analysed,
            'analysedPaths' => $paths,
            'analysedPathsFromConfig' => $configured,
            'excludePaths' => $excludes,
            'collected' => $collected === null ? null : $data,
            'onlyFiles' => $onlyFiles,
            'collectedDataRuleSkipped' => $skipped,
            'expected' => [
                'errors' => array_map(static fn (Error $x) => [$x->getFile(), $x->getLine(), $x->getMessage()], $result->getNotIgnoredErrors()),
                'stringErrors' => $result->getOtherIgnoreMessages(),
            ],
        ]);
    }
    $out = ['ignoreErrors' => json_decode(str_replace(addcslashes($dir, '/'), '{dir}', json_encode(tagged($entries))), true), 'errors' => [['Foo', 'a.php', 1], ['Bar', 'b.php', 2]], 'cases' => $out];
    file_put_contents($fixtures.'/ignore-judge.json', json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)."\n");
    echo 'ignore-judge.json: ', count($cases), " cases\n";
}
