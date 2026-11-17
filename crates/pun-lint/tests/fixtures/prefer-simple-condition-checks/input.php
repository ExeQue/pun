<?php

namespace App;

class Conditions
{
    public function identicalTrue(array $items, string $value): bool
    {
        return in_array($value, $items, true) === true;
    }

    public function notIdentical(bool $a, bool $b): bool
    {
        return ! $a === $b;
    }

    public function conditions(int $bar): bool
    {
        return !($bar > 0);
    }
}
