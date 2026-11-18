<?php

namespace App;

class Example
{
    public function run($name)
    {
        $a = constant(Example::class . '::FOO');
        $b = constant('App\Example::' . $name);
        $c = constant(Example::BAR . '::' . $name);
        $d = constant(Example::class . ':' . $name);
        $e = constant(Example::class . '::' . $name . 'X');
        $f = constant(Example::class + '::' . $name);
        $g = constant($name);
        $h = constant(Example::class . '::' . $name, 1);
        $i = defined(Example::class . '::' . $name);
    }
}
