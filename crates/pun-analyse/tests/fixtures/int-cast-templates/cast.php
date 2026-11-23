<?php

namespace App;

use Closure;

/**
 * @template T of int|array<string, int>
 * @param  Closure(): T  $count
 * @return T|null
 */
function count(Closure $count, bool $skip): int|array|null
{
    if ($skip) {
        return null;
    }

    $value = $count();
    if (! is_array($value)) {
        \PHPStan\dumpType((int) $value);
    }

    return is_array($value) ? $value : (int) $value;
}

/**
 * @template T of int
 * @param  T  $v
 * @return T
 */
function same(int $v): int
{
    \PHPStan\dumpType((int) $v);

    return (int) $v;
}

/**
 * @template T of positive-int
 * @param  T  $v
 * @return T
 */
function positive(int $v): int
{
    \PHPStan\dumpType((int) $v);

    return (int) $v;
}

/**
 * @template T of 5
 * @param  T  $v
 * @return T
 */
function five(int $v): int
{
    \PHPStan\dumpType((int) $v);

    return (int) $v;
}

/**
 * @template T of string
 * @param  T  $v
 * @return T
 */
function text(string $v): string
{
    \PHPStan\dumpType((int) $v);

    return (string) (int) $v;
}

/**
 * @template T of int|string
 * @param  T  $v
 * @return T
 */
function either(int|string $v): int|string
{
    \PHPStan\dumpType((int) $v);

    return $v;
}
