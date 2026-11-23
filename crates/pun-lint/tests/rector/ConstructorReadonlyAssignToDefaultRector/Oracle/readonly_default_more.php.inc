<?php

namespace App;

final class Many
{
    public readonly int $a;
    public readonly int $b;
    public readonly string $c;
    public readonly float $d;
    public readonly ?string $e;
    public readonly int $f;
    public readonly int $g = 1;
    public readonly int $h, $i;
    public static int $j;

    public function __construct(int $f)
    {
        $this->a = -1;
        $this->b = +2;
        $this->c = __CLASS__;
        $this->d = PHP_FLOAT_EPSILON;
        $this->e = null;
        $this->f = $f;
        $this->f = 3;
        $this->g = 2;
        $this->h = 4;
        if ($f) {
            $this->i = 5;
        }
        self::$j = 6;
        $this->c .= 'x';
    }
}

$object = new class {
    public readonly array $list;

    public function __construct()
    {
        $this->list = [];
        $this->other = 1;
    }
};

final class Abstracted
{
    public readonly int $a;

    public function __construct()
    {
        $this->a = 1 + 1;
    }
}

final class NoConstructor
{
    public readonly int $a;
}
-----
<?php

namespace App;

final class Many
{
    public readonly int $a = -1;
    public readonly int $b = +2;
    public readonly string $c = __CLASS__;
    public readonly float $d = PHP_FLOAT_EPSILON;
    public readonly ?string $e = null;
    public readonly int $f;
    public readonly int $g = 1;
    public readonly int $h, $i;
    public static int $j;

    public function __construct(int $f)
    {
        $this->f = $f;
        $this->f = 3;
        $this->g = 2;
        $this->h = 4;
        if ($f) {
            $this->i = 5;
        }
        self::$j = 6;
        $this->c .= 'x';
    }
}

$object = new class {
    public readonly array $list;

    public function __construct()
    {
        $this->list = [];
        $this->other = 1;
    }
};

final class Abstracted
{
    public readonly int $a;

    public function __construct()
    {
        $this->a = 1 + 1;
    }
}

final class NoConstructor
{
    public readonly int $a;
}
