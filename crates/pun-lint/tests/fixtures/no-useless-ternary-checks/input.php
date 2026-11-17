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
        return ($a !== $b) ? $a : $b;
    }

    public function unnecessary(string $foo): bool
    {
        return $foo === 'bar' ? true : false;
    }
}
