<?php

namespace App;

final class Negated
{
    public function ands(int $a, int $b): bool
    {
        return !($a > 20 && $b <= 50);
    }

    public function deMorgan(int $a, int $b): bool
    {
        return $a <= 20 && $b > 50;
    }
}
