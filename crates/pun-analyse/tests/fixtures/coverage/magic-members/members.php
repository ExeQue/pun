<?php

namespace MagicMembers;

/** @template T of bool = false */
class Pending
{
    /** @return $this */
    public function withHeader(string $name)
    {
        return $this;
    }

    public function fluent(): static
    {
        return $this;
    }
}

/** @mixin Pending */
class Factory
{
    /** @param array<mixed> $arguments */
    public static function __callStatic(string $name, array $arguments): mixed
    {
        return null;
    }
}

/** @mixin Factory */
class Facade
{
    /** @param array<mixed> $arguments */
    public static function __callStatic(string $name, array $arguments): mixed
    {
        return null;
    }
}

final class Magic
{
    public function __get(string $name): self
    {
        return $this;
    }

    public function __set(string $name, int $value): void
    {
    }
}

function staticMixinCalls(): void
{
    \PHPStan\dumpType(Factory::withHeader('a'));
    \PHPStan\dumpType(Factory::fluent());
    \PHPStan\dumpType(Facade::withHeader('a'));
    \PHPStan\dumpType(Facade::fluent());
}

function undocumentedMagicProperty(Magic $magic): void
{
    \PHPStan\dumpType($magic->name instanceof Magic);
    $magic->name = 'text';
}

class Forwarding
{
    /** @return array<string> */
    protected function hidden(): array
    {
        return [];
    }

    /** @param array<mixed> $arguments */
    public function __call(string $name, array $arguments): mixed
    {
        return null;
    }
}

function inaccessibleMethodWithMagicCall(Forwarding $forwarding): void
{
    \PHPStan\dumpType($forwarding->hidden());
}
