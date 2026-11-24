<?php

namespace App;

final class Helper
{
    public function one(int $x): int
    {
        return $x;
    }

    public static function two(int $a, int $b): int
    {
        return $a + $b;
    }
}

final class Shapes
{
    public function run(Helper $helper, array $items): void
    {
        usort($items, function ($a, $b) {
            return Helper::two($a, $b);
        });
        array_map(function ($x) use ($helper) {
            return $helper->one($x);
        }, $items);
        array_map(static function ($x) use ($helper) {
            return $helper->one($x);
        }, $items);
        array_map(function ($x) use ($helper) {
            $y = 1;
            return $helper->one($x);
        }, $items);
        array_map(function ($x) use ($helper) {
            return;
        }, $items);
        $f = function ($x) use ($helper) {
            return $helper->one($x);
        };
        array_map(function ($x) {
            return strtoupper(
                $x,
            );
        }, $items);
    }
}
-----
<?php

namespace App;

final class Helper
{
    public function one(int $x): int
    {
        return $x;
    }

    public static function two(int $a, int $b): int
    {
        return $a + $b;
    }
}

final class Shapes
{
    public function run(Helper $helper, array $items): void
    {
        usort($items, Helper::two(...));
        array_map($helper->one(...), $items);
        array_map($helper->one(...), $items);
        array_map(function ($x) use ($helper) {
            $y = 1;
            return $helper->one($x);
        }, $items);
        array_map(function ($x) use ($helper) {
            return;
        }, $items);
        $f = function ($x) use ($helper) {
            return $helper->one($x);
        };
        array_map(strtoupper(
            ...
        ), $items);
    }
}
