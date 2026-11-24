<?php

namespace ArraySum;

use function PHPStan\dumpType;

/**
 * @param list<int|null> $nullable
 * @param list<int|bool> $bools
 * @param list<int<1, 5>|null> $range
 * @param non-empty-list<int|null> $nonEmpty
 * @param array<string, int|null> $map
 * @param list<int|float|null> $numbers
 * @param list<float|null> $floats
 * @param list<int<-5, -1>> $negative
 * @param list<0> $zeros
 * @param array{int, ?int} $shape
 * @param array{1, 2} $constant
 * @param array{} $empty
 * @param array{a: int, b?: float} $optional
 * @param array{true, false, bool} $flags
 * @param array{1}|list<int<5, 9>> $either
 * @param list<int>|false $orFalse
 * @param list<numeric-string> $strings
 * @param array<string, int> $grown
 * @param list<int<1, 3>> $ints
 * @param T $template
 * @template T of array<int>
 */
function f(array $nullable, array $bools, array $range, array $nonEmpty, array $map, array $numbers, array $floats, array $negative, array $zeros, array $shape, array $constant, array $empty, array $optional, array $flags, array $either, $orFalse, array $strings, array $grown, array $ints, array $template): void
{
    dumpType(array_sum($nullable));
    dumpType(array_sum($bools));
    dumpType(array_sum($range));
    dumpType(array_sum($nonEmpty));
    dumpType(array_sum($map));
    dumpType(array_sum($numbers));
    dumpType(array_sum($floats));
    dumpType(array_sum($negative));
    dumpType(array_sum($zeros));
    dumpType(array_sum($shape));
    dumpType(array_sum($constant));
    dumpType(array_sum($empty));
    dumpType(array_sum($optional));
    dumpType(array_sum($flags));
    dumpType(array_sum($either));
    if ($orFalse !== false) {
        dumpType(array_sum($orFalse));
    }
    dumpType(array_sum($strings));
    $grown['a'] = 1;
    dumpType(array_sum($grown));
    $list = [1, 2];
    $list[] = 3;
    dumpType(array_sum($list));
    $ints[] = 5;
    dumpType(array_sum($ints));
    $maybe = [];
    if (rand(0, 1) === 1) {
        $maybe['a'] = 1;
    }
    $maybe['b'] = null;
    dumpType(array_sum($maybe));
    dumpType(array_sum($template));
}
