<?php

namespace App;

trait NonSerializable
{
    public function __serialize(): array
    {
        throw new \LogicException('no');
    }

    public function other(): array
    {
        return [];
    }
}

final class A
{
    use NonSerializable;
}

final class B
{
    /** @var list<string> */
    private array $names = [];

    public function __serialize(): array
    {
        return ['names' => $this->names];
    }

    public function __sleep(): array
    {
        return ['names'];
    }

    public function plain(): array
    {
        return $this->names;
    }
}
