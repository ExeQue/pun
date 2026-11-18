<?php

namespace PunLocalMacro;

use Illuminate\Support\Str;

use function PHPStan\Testing\assertType;

function test(): void
{
    assertType('int', Str::punLocal());
}
