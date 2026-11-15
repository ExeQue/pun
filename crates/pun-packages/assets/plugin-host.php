<?php

// pun's Composer plugin host: runs a project's Composer plugins on Composer's own classes.
//
// argv: the pinned composer.phar; `init` or `late`; then the names of the plugins pun runs itself (kept out of here).
// The host builds a real Composer object from the project in the working directory, as `composer install` would, so
// Composer's PluginManager activates the allowed plugins installed in vendor/ and Composer dispatches `init`. With
// `late`, the plugins were installed by this command, after Composer would have started: Composer activates such a
// plugin as it installs it and never sends it `init`, so the host loads them only after `init`. pun then sends one JSON request per line on
// stdin, and the host answers each on stdout after the token in PUN_PLUGIN_HOST_TOKEN, at the end of a line, so the
// plugins' own output passes through untouched:
//
//   {"op": "dispatch", "event": "post-autoload-dump", "dev": true, "phase": "before"}
//       Runs the event's plugin listeners: "before" those at priority 0 and up, which Composer runs before the
//       project's scripts, "after" the negative ones. pun runs the scripts between the two.
//   {"op": "sync"}
//       Reads vendor/composer/installed.json again and activates the plugins installed or updated since, as
//       Composer's PluginInstaller does while it installs them.
//
// The answer is {} or {"error": "..."}.

Phar::loadPhar($argv[1], 'composer.phar');
require 'phar://composer.phar/src/bootstrap.php';

use Composer\EventDispatcher\EventDispatcher;
use Composer\Factory;
use Composer\IO\ConsoleIO;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginManager;
use Composer\Util\Platform;
use Symfony\Component\Console\Helper\HelperSet;
use Symfony\Component\Console\Input\ArrayInput;

final class PunPluginManager extends PluginManager
{
    /** @var list<string> */
    public static $skipped = [];

    /** @var bool */
    public static $late = false;

    public function loadInstalledPlugins(): void
    {
        if (!self::$late) {
            parent::loadInstalledPlugins();
        }
    }

    public function loadLate(): void
    {
        self::$late = false;
        parent::loadInstalledPlugins();
    }

    public function isPluginAllowed(string $package, bool $isGlobalPlugin, bool $optional = false, bool $prompt = true): bool
    {
        if (!$isGlobalPlugin && in_array($package, self::$skipped, true)) {
            return false;
        }

        return parent::isPluginAllowed($package, $isGlobalPlugin, $optional, $prompt);
    }
}

final class PunFactory extends Factory
{
    protected function createPluginManager(IOInterface $io, \Composer\Composer $composer, ?\Composer\PartialComposer $globalComposer = null, $disablePlugins = false): PluginManager
    {
        return new PunPluginManager($io, $composer, $globalComposer, $disablePlugins);
    }
}

PunPluginManager::$late = $argv[2] === 'late';
PunPluginManager::$skipped = array_slice($argv, 3);
$token = (string) getenv('PUN_PLUGIN_HOST_TOKEN');

function answer(string $token, array $reply): void
{
    fwrite(STDOUT, $token.json_encode((object) $reply)."\n");
    fflush(STDOUT);
}

$input = new ArrayInput([]);
$input->setInteractive(false);
$io = new ConsoleIO($input, Factory::createOutput(), new HelperSet());
\Composer\Util\ErrorHandler::register($io);

try {
    // pun runs the project's scripts itself, so the dispatcher runs only the plugins' listeners.
    $composer = (new PunFactory())->createComposer($io, getcwd().'/'.Factory::getComposerFile(), false, getcwd(), true, true);
    $composer->getPluginManager()->loadLate();
} catch (\Throwable $e) {
    answer($token, ['error' => $e->getMessage()]);
    exit(1);
}
answer($token, []);

$dispatcher = $composer->getEventDispatcher();
// Takes the listeners of the other phase out of the event, and gives them back.
$split = \Closure::bind(function (string $name, bool $before): array {
    $out = [];
    foreach ($this->listeners[$name] ?? [] as $priority => $listeners) {
        if (($priority >= 0) !== $before) {
            $out[$priority] = $listeners;
            unset($this->listeners[$name][$priority]);
        }
    }

    return $out;
}, $dispatcher, EventDispatcher::class);
$join = \Closure::bind(function (string $name, array $out): void {
    foreach ($out as $priority => $listeners) {
        $this->listeners[$name][$priority] = array_merge($this->listeners[$name][$priority] ?? [], $listeners);
    }
}, $dispatcher, EventDispatcher::class);

$repo = $composer->getRepositoryManager()->getLocalRepository();
$pm = $composer->getPluginManager();

while (($line = fgets(STDIN)) !== false) {
    $request = json_decode($line, true);
    try {
        if ($request['op'] === 'dispatch') {
            Platform::putEnv('COMPOSER_DEV_MODE', $request['dev'] ? '1' : '0');
            $out = $split($request['event'], $request['phase'] === 'before');
            try {
                $dispatcher->dispatchScript($request['event'], $request['dev']);
            } finally {
                $join($request['event'], $out);
            }
        } elseif ($request['op'] === 'sync') {
            $before = [];
            foreach ($repo->getPackages() as $package) {
                $before[$package->getName()] = $package;
            }
            $repo->reload();
            foreach ($repo->getPackages() as $package) {
                if ($package->getType() !== 'composer-plugin' && $package->getType() !== 'composer-installer') {
                    continue;
                }
                $old = $before[$package->getName()] ?? null;
                if ($old !== null && $old->getVersion() === $package->getVersion() && $old->getDistReference() === $package->getDistReference()) {
                    continue;
                }
                if ($old !== null) {
                    $pm->deactivatePackage($old);
                }
                $pm->registerPackage($package, true);
            }
        }
        answer($token, []);
    } catch (\Throwable $e) {
        answer($token, ['error' => $e->getMessage()]);
    }
}
