<?php

// symfony/cache's ValueWrapper: a class name that is the single byte 0xA9, not valid UTF-8.

class ©
{
    private const LIMIT = 1;

    public function __construct(public readonly mixed $value)
    {
    }

    public function limit(): int
    {
        return self::LIMIT;
    }
}

// symfony/cache's CacheItem instantiates it through a string constant.
final class Packer
{
    private const VALUE_WRAPPER = "\xA9";

    public static function pack(mixed $value): object
    {
        return new (self::VALUE_WRAPPER)($value);
    }
}
