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

    public function byRef(array &$x): void
    {
    }

    public function optional(int $x, int $y = 0): int
    {
        return $x;
    }
}

final class Shapes
{
    private Helper $helper;

    public function run(Helper $helper, array $items): void
    {
        usort($items, fn ($a, $b) => Helper::two($a, $b));
        usort($items, fn ($a, $b) => Helper::two($b, $a));
        array_map(fn ($x) => $helper->one($x), $items);
        array_map(fn ($x) => $this->helper->one($x), $items);
        array_map(fn ($x) => $helper->optional($x), $items);
        array_map(fn ($x) => \strtoupper($x), $items);
        array_filter($items, fn ($x) => is_int($x));
        array_map(fn ($x) => $x->one($x), $items);
        array_map(fn (&$x) => $helper->one($x), $items);
        array_map(fn ($x) => $helper->one(x: $x), $items);
        call_user_func(fn ($x) => $helper->one($x), 1);
        $this->take(fn ($x) => $helper->one($x));
        $f = fn ($x) => $helper->one($x);
        $g = [fn ($x) => $helper->one($x)];
        array_map(static fn ($x) => $this->helper->one($x), $items);
        array_map(fn (...$x) => Helper::two(...$x), $items);
        array_map(fn ($x) => (new Helper())->one($x), $items);
        array_map(fn () => time(), $items);
        array_map(fn ($x) => $helper->missing($x), $items);
    }

    public static function stat(array $items): void
    {
        array_map(fn ($x) => $this->helper->one($x), $items);
    }

    private function take(callable $c): void
    {
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

    public function byRef(array &$x): void
    {
    }

    public function optional(int $x, int $y = 0): int
    {
        return $x;
    }
}

final class Shapes
{
    private Helper $helper;

    public function run(Helper $helper, array $items): void
    {
        usort($items, Helper::two(...));
        usort($items, fn ($a, $b) => Helper::two($b, $a));
        array_map($helper->one(...), $items);
        array_map($this->helper->one(...), $items);
        array_map(fn ($x) => $helper->optional($x), $items);
        array_map(\strtoupper(...), $items);
        array_filter($items, is_int(...));
        array_map(fn ($x) => $x->one($x), $items);
        array_map(fn (&$x) => $helper->one($x), $items);
        array_map(fn ($x) => $helper->one(x: $x), $items);
        call_user_func(fn ($x) => $helper->one($x), 1);
        $this->take(fn ($x) => $helper->one($x));
        $f = fn ($x) => $helper->one($x);
        $g = [$helper->one(...)];
        array_map(static fn ($x) => $this->helper->one($x), $items);
        array_map(fn (...$x) => Helper::two(...$x), $items);
        array_map(fn ($x) => (new Helper())->one($x), $items);
        array_map(time(...), $items);
        array_map(fn ($x) => $helper->missing($x), $items);
    }

    public static function stat(array $items): void
    {
        array_map(fn ($x) => $this->helper->one($x), $items);
    }

    private function take(callable $c): void
    {
    }
}
