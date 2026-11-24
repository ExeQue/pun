<?php

namespace App;

class Helper
{
    public function run(): void
    {
    }

    public static function make(): void
    {
    }

    protected function hidden(): void
    {
    }
}

class Needs
{
    public function __construct(int $x)
    {
    }

    public static function make(): void
    {
    }
}

final class Shapes
{
    private Helper $helper;

    public function go(Helper $helper, ?Helper $maybe, callable|array $either): void
    {
        $a = [$helper, 'run'];
        $b = array($helper, 'run');
        $c = [$this->helper, 'run'];
        $d = [Helper::class, 'make'];
        $e = [self::class, 'go'];
        $f = [static::class, 'local'];
        $g = [__CLASS__, 'local'];
        $h = [$maybe, 'run'];
        $i = [$helper, 'hidden'];
        $j = [$helper, 'missing'];
        $k = [Needs::class, 'make'];
        $l = [$this, 'local'];
        $m = ['callback' => [$this, 'local']];
        $n = array_map([$this, 'local'], []);
        $o = in_array([$this, 'local'], []);
        $p = call_user_func(...[[$this, 'local']]);
        $q = [$this, '__construct'];
        $r = [$this, 'LOCAL'];
        register_shutdown_function([$this, 'local']);
        $s = usort($list, array: [$this, 'local']);
    }

    private function local(): void
    {
    }
}
-----
<?php

namespace App;

class Helper
{
    public function run(): void
    {
    }

    public static function make(): void
    {
    }

    protected function hidden(): void
    {
    }
}

class Needs
{
    public function __construct(int $x)
    {
    }

    public static function make(): void
    {
    }
}

final class Shapes
{
    private Helper $helper;

    public function go(Helper $helper, ?Helper $maybe, callable|array $either): void
    {
        $a = $helper->run(...);
        $b = $helper->run(...);
        $c = $this->helper->run(...);
        $d = Helper::make(...);
        $e = self::go(...);
        $f = static::local(...);
        $g = [__CLASS__, 'local'];
        $h = [$maybe, 'run'];
        $i = [$helper, 'hidden'];
        $j = [$helper, 'missing'];
        $k = [Needs::class, 'make'];
        $l = $this->local(...);
        $m = ['callback' => [$this, 'local']];
        $n = array_map($this->local(...), []);
        $o = in_array($this->local(...), []);
        $p = call_user_func(...[$this->local(...)]);
        $q = [$this, '__construct'];
        $r = $this->LOCAL(...);
        register_shutdown_function([$this, 'local']);
        $s = usort($list, array: [$this, 'local']);
    }

    private function local(): void
    {
    }
}
