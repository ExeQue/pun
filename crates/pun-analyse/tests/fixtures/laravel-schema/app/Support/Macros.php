<?php

namespace App\Support;

use Illuminate\Support\Str;

final class Macros
{
    public static function register(): void
    {
        Str::macro('shout', fn (string $value): string => $value.'!');
    }
}
