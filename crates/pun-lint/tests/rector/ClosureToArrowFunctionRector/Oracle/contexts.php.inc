<?php

namespace Pun\Oracle;

final class ArrowContexts
{
    public function run($items, $flag)
    {
        $a = $flag ? function ($x) {
            return $x + 1;
        } : null;
        $b = $flag ? null : function ($x) {
            return $x + 1;
        };
        $c = $flag ?: function ($x) {
            return $x;
        };
        $d = $flag ?? function () {
            return 1;
        };
        throw function () {
            return 1;
        };
    }

    public function comments($items)
    {
        return function () {
            // explain
            return 1;
        };
    }

    public function trailing()
    {
        $f = function () {
            return 1;
            // after
        };
        $g = function () use ($items) {
            return compact(['other', ['items']]);
        };
        $h = function &(array &$x) use ($y): array {
            return $x;
        };
        $i = function () use (&$total) {
            return fn () => $total;
        };
    }
}
-----
<?php

namespace Pun\Oracle;

final class ArrowContexts
{
    public function run($items, $flag)
    {
        $a = $flag ? fn($x) => $x + 1 : null;
        $b = $flag ? null : (fn($x) => $x + 1);
        $c = $flag ?: (fn($x) => $x);
        $d = $flag ?? (fn() => 1);
        throw fn() => 1;
    }

    public function comments($items)
    {
        return // explain
        fn() => 1;
    }

    public function trailing()
    {
        $f = function () {
            return 1;
            // after
        };
        $g = function () use ($items) {
            return compact(['other', ['items']]);
        };
        $h = (fn&(array &$x): array => $x);
        $i = (fn() => fn () => $total);
    }
}
