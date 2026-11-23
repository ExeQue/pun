<?php

namespace App;

use Countable;

function go($foo, $bar, array $items)
{
    $a = $foo instanceof Countable || is_array($foo);
    $b = (is_array($foo)) || ($foo instanceof \Countable);
    $c = is_array($foo) || $bar instanceof Countable;
    $d = is_array($foo) or $foo instanceof Countable;
    $e = is_array(...$items) || $foo instanceof Countable;
    $f = \is_array($foo[0]) || $foo[0] instanceof \countable;
    $g = is_array($foo) || $foo instanceof \Traversable;
}
-----
<?php

namespace App;

use Countable;

function go($foo, $bar, array $items)
{
    $a = is_countable($foo);
    $b = is_countable($foo);
    $c = is_array($foo) || $bar instanceof Countable;
    $d = is_array($foo) or $foo instanceof Countable;
    $e = is_array(...$items) || $foo instanceof Countable;
    $f = \is_array($foo[0]) || $foo[0] instanceof \countable;
    $g = is_array($foo) || $foo instanceof \Traversable;
}
