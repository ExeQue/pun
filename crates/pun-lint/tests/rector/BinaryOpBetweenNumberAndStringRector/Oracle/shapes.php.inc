<?php

class Shapes
{
    public function run(int $i, float $f, string $s)
    {
        $a = $i + 'abc';
        $b = 'abc' - $f;
        $c = $i == 'x';
        $d = 'x' < $f;
        $e = $i + '12';
        $g = $i . 'abc';
        $h = $i + "abc";
        $j = 3 % 'abc';
        $k = $i ** 'x';
        $l = 'abc' + 'def';
        $m = __LINE__ + 'a';
        $n = $i + __FILE__;
    }
}
-----
<?php

class Shapes
{
    public function run(int $i, float $f, string $s)
    {
        $a = $i + 0;
        $b = 0.0 - $f;
        $c = $i == 0;
        $d = 0.0 < $f;
        $e = $i + '12';
        $g = $i . 'abc';
        $h = $i + 0;
        $j = 3 % 0;
        $k = $i ** 0;
        $l = 'abc' + 'def';
        $m = __LINE__ + 0;
        $n = $i + 0;
    }
}
