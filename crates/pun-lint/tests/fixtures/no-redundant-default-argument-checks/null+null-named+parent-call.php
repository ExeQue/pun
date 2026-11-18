<?php

namespace App;

class DefaultsParent
{
    public function __construct(string $message, array $items = [])
    {
    }
}

final class Defaults extends DefaultsParent
{
    public function __construct(string $message)
    {
        parent::__construct($message);
    }

    public static function withNull(?string $a = null, ?string $b = null): void
    {
    }

    public function null(): void
    {
        self::withNull('a');
    }

    public function nullNamed(): void
    {
        self::withNull();
    }
}
