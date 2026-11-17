<?php

namespace App;

class Ternaries
{
    public function useless(bool $value): bool
    {
        return $value ?: false;
    }

    public function tautology(string $a, string $b): string
    {
        return $a;
    }

    public function unnecessary(string $foo): bool
    {
        return $foo === 'bar';
    }
}
