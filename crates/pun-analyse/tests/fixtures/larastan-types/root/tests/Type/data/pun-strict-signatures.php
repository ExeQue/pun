<?php

declare(strict_types=1);

namespace PunStrictSignatures;

class Base
{
    /** @return array<string, mixed> */
    public function toArray(): array
    {
        return [];
    }

    /** @param array<int, string>|\Stringable|string $columns */
    public function select($columns): void
    {
    }

    /** @return list<int> */
    public static function ids(): array
    {
        return [];
    }
}

class Child extends Base
{
    /** @return array<mixed> */
    public function toArray(): array
    {
        return [];
    }

    /** @param array<int, string>|string $columns */
    public function select($columns): void
    {
    }

    /** @return array<int> */
    public static function ids(): array
    {
        return [];
    }
}

/** @template T */
abstract class Factory
{
    /** @return static */
    public function configure()
    {
        return $this;
    }
}

/** @extends Factory<int> */
final class IntFactory extends Factory
{
    public function configure(): self
    {
        return $this;
    }
}

final class Tags
{
    /** @return array<int, string> */
    private function names(): array
    {
        return ['a'];
    }

    /** @return list<string> */
    private function list(): array
    {
        return ['a'];
    }

    public function run(): int
    {
        /** @var array<int, int> $names */
        $names = $this->names();
        /** @var array<int, string> $list */
        $list = $this->list();

        return count($names) + count($list);
    }
}
