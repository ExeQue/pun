<?php

// The part of myclabs/php-enum's `MyCLabs\Enum\Enum` the rule reads: its constructor and `from()`.

namespace MyCLabs\Enum;

abstract class Enum
{
    public function __construct($value)
    {
    }

    public static function from($value): static
    {
        return new static($value);
    }
}
