<?php

namespace Acme\Denied;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

final class Plugin implements PluginInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        file_put_contents(getcwd().'/events.log', "denied activated\n", FILE_APPEND);
    }

    public function deactivate(Composer $composer, IOInterface $io): void {}

    public function uninstall(Composer $composer, IOInterface $io): void {}
}
