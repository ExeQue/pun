<?php

namespace App;

final class Helpers
{
    public static function quarterName(int $quarter): string
    {
        return 'Q'.$quarter;
    }
}
