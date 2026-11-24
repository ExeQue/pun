<?php

class Shapes
{
    public $items = '';
    public $other = '';
    public static $list = '';
    public $kept = '';

    public function run()
    {
        $this->items[] = 1;
        static::$list[] = 2;
        $this->kept['a'] = 3;
        $local = '';
        $local[] = 1;
        $later = '';
        $later = 'text';
        $later[] = 1;
        $arr = '';
        $arr[] = 1;
        $arr = [2];
        $fn = function () {
            $inner = '';
            $inner[] = 1;
        };
        $arrow = '';
        $g = fn () => $arrow[] = 1;
    }
}

$top = '';
$top[] = 'x';
-----
<?php

class Shapes
{
    public $items = [];
    public $other = '';
    public static $list = [];
    public $kept = '';

    public function run()
    {
        $this->items[] = 1;
        static::$list[] = 2;
        $this->kept['a'] = 3;
        $local = [];
        $local[] = 1;
        $later = '';
        $later = 'text';
        $later[] = 1;
        $arr = '';
        $arr[] = 1;
        $arr = [2];
        $fn = function () {
            $inner = [];
            $inner[] = 1;
        };
        $arrow = [];
        $g = fn () => $arrow[] = 1;
    }
}

$top = [];
$top[] = 'x';
