<?php

namespace App\Providers;

use Illuminate\Support\ServiceProvider;
use Illuminate\Support\Str;
use PunLocalMacros\LocalHelper;

require_once __DIR__.'/../../resources/pun/local-helper.php';

class PunLocalServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        Str::macro('punLocal', [LocalHelper::class, 'make']);
    }
}
