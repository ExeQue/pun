<?php

namespace App;

/**
 * @template TKey of array-key
 * @template TValue
 * @implements \IteratorAggregate<TKey, TValue>
 */
final class Coll implements \IteratorAggregate
{
    /**
     * @template K of array-key
     * @template V
     * @param callable(TValue, TKey): array<K, V> $callback
     * @return self<K, V>
     */
    public function mapWithKeys(callable $callback): self
    {
        throw new \LogicException();
    }

    /**
     * @param array<array-key, TKey>|string $keys
     * @return self<TKey, TValue>
     */
    public function only(array|string $keys): self
    {
        return $this;
    }

    /** @return \ArrayIterator<TKey, TValue> */
    public function getIterator(): \ArrayIterator
    {
        return new \ArrayIterator([]);
    }
}

/**
 * @param Coll<int, string> $names
 * @param list<string> $wanted
 * @return Coll<string, string>
 */
function widened(Coll $names, array $wanted): Coll
{
    $byKey = $names->mapWithKeys(fn (string $name): array => [mb_strtolower($name) => $name]);
    \PHPStan\dumpType($byKey);

    return $byKey->only($wanted);
}

/**
 * @param Coll<int, string> $names
 * @return Coll<lowercase-string, string>
 */
function kept(Coll $names): Coll
{
    $byKey = $names->mapWithKeys(fn (string $name): array => [mb_strtolower($name) => $name]);
    \PHPStan\dumpType($byKey);

    return $byKey->only('a');
}

/**
 * @param Coll<int, string> $names
 * @param Coll<int, string> $posts
 */
function fromAnotherSite(Coll $names, Coll $posts): void
{
    $wantedByPost = $posts->mapWithKeys(fn (string $body, int $id): array => [$id => explode(' ', $body)]);
    $byKey = $names->mapWithKeys(fn (string $name): array => [mb_strtolower($name) => $name]);
    \PHPStan\dumpType($byKey);
    foreach ($wantedByPost as $wanted) {
        \PHPStan\dumpType($wanted);
        \PHPStan\dumpType($byKey->only($wanted));
    }
}

/**
 * @param Coll<int, string> $names
 * @param list<int> $wanted
 */
function rejected(Coll $names, array $wanted): void
{
    $byKey = $names->mapWithKeys(fn (string $name): array => [mb_strtolower($name) => $name]);
    \PHPStan\dumpType($byKey);
    \PHPStan\dumpType($byKey->only($wanted));
}
