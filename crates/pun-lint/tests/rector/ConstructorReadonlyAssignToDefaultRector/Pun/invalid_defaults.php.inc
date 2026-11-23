<?php

namespace App;

// Rector moves `static::X` and an interpolated string into the property defaults, which PHP does not accept as
// defaults; Pun leaves those assignments in the constructor.

class Invalid
{
    public const X = 1;
    public readonly int $a;
    public readonly string $b;

    public function __construct(string $name)
    {
        $this->a = static::X;
        $this->b = "hello {$name}";
    }
}
