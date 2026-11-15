<?php

namespace Acme\Recorder;

use Composer\Composer;
use Composer\EventDispatcher\Event;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;

final class Plugin implements PluginInterface, EventSubscriberInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        self::log('activate '.$composer->getPackage()->getName());
    }

    public function deactivate(Composer $composer, IOInterface $io): void {}

    public function uninstall(Composer $composer, IOInterface $io): void {}

    public static function getSubscribedEvents(): array
    {
        return [
            'init' => 'mid',
            'pre-install-cmd' => 'mid',
            'pre-update-cmd' => 'mid',
            'pre-autoload-dump' => 'mid',
            'post-autoload-dump' => [['early', 10], ['mid', 0], ['late', -10]],
            'post-install-cmd' => 'mid',
            'post-update-cmd' => 'mid',
        ];
    }

    public function early(Event $e): void { self::log($e->getName().' plugin 10 '.getenv('COMPOSER_DEV_MODE')); }
    public function mid(Event $e): void
    {
        self::log($e->getName().' plugin 0 '.getenv('COMPOSER_DEV_MODE'));
        $fail = @file_get_contents(getcwd().'/fail');
        if ($fail === $e->getName().' throw') {
            throw new \RuntimeException('the recorder failed');
        }
        if ($fail === $e->getName().' echo') {
            echo "the recorder says hello\n";
        }
        if ($fail === $e->getName().' exit') {
            exit(3);
        }
    }
    public function late(Event $e): void { self::log($e->getName().' plugin -10 '.getenv('COMPOSER_DEV_MODE')); }

    private static function log(string $line): void
    {
        file_put_contents(getcwd().'/events.log', $line."\n", FILE_APPEND);
    }
}
