<?php

namespace App;

class C
{
    public function a()
    {
        $a = (string) config('a', 'default');
        $b = (int) \config('b');
        $c = (object) config('c');
        $d = (bool) config(['x' => 1]);
        $e = (float) (config('e'));
        $f = (array) config('f') + [];
        $g = (integer) config('g');
        $h = (boolean) config('h');
        $i = (double) config('i');
        return -(int) config('j');
    }
}
-----
<?php

namespace App;

class C
{
    public function a()
    {
        $a = (string) config('a', 'default');
        $b = config()->integer('b');
        $c = (object) config('c');
        $d = config()->boolean(['x' => 1]);
        $e = config()->float('e');
        $f = config()->array('f') + [];
        $g = config()->integer('g');
        $h = config()->boolean('h');
        $i = config()->float('i');
        return -config()->integer('j');
    }
}
