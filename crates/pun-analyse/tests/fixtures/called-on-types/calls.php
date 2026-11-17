<?php

namespace CalledOn;

use function PHPStan\dumpType;

trait Conditions
{
    /**
     * @param callable($this): void $callback
     */
    public function whenThis(callable $callback): void {}
}

/** @template T of \Countable */
class Box
{
    use Conditions;

    /**
     * @param callable($this): void $this_
     * @param array<static> $statics
     * @param callable(self): void $self
     */
    public function take(callable $this_, array $statics, callable $self): void {}

    /**
     * @param callable(T): void $callback
     */
    public function each(callable $callback): void {}
}

/** @template-covariant T */
class Pair {}

final class Calls
{
    /**
     * @param Box<\ArrayObject<int, int>> $box
     * @param Box<\ArrayObject<int, int>|\SplStack<string>> $either
     */
    public function calls(Box $box, Box $either): void
    {
        $box->take(5, 5, 5);
        $box->whenThis(5);
        $box->whenThis(function (int $value): void {});
        $box->whenThis(function (Box $box): void {});
        dumpType(function (Box $box): void {});
        dumpType(function (Box|Pair $value): void {});
        $either->each(function (\Countable $value): void {
            dumpType($value);
        });
        $either->each(fn (\Countable|string $value) => dumpType($value));
        $either->each(function (mixed $value): void {
            dumpType($value);
        });
    }
}

/**
 * @template TKey of array-key
 * @template TValue
 */
class Bag
{
    /**
     * @param callable(TValue): bool $callback
     */
    public function reject(callable $callback): void {}
}

final class Unions
{
    /**
     * @param Bag<int, \DateTime>|Bag<int, \DateTimeZone> $bags
     */
    public function calls(Bag $bags): void
    {
        $bags->reject(function (\DateTime|\DateTimeZone $value): bool {
            dumpType($value);
            return true;
        });
        $bags->reject(function ($value): bool {
            dumpType($value);
            return true;
        });
    }
}

/**
 * @template TKey of array-key
 * @template TValue
 */
class Stack
{
    /**
     * @template TFlatKey of array-key
     * @template TFlatValue
     * @param callable(TValue): array<TFlatKey, TFlatValue> $callback
     * @return static<TFlatKey, TFlatValue>
     */
    public function flatMap(callable $callback)
    {
        throw new \LogicException();
    }
}

final class GenericStatics
{
    /**
     * @param Stack<string, float> $floats
     */
    public function calls(Stack $floats, array $values): void
    {
        dumpType($floats->flatMap(fn (float $value): array => $values));
        dumpType($floats->flatMap(fn (float $value): array => [$value]));
    }
}
