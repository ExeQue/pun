<?php

function run($items)
{
    [$key, $value] = each($items);
    list($a, $b) = @each($items);
    list('x' => $k, 'y' => $v) = each($items);
    list(, ) = each($items);
    list($only) = each($items);
    if ($items) list($c, $d) = each($items);
    $x = list($e, $f) = each($items);
}
-----
<?php

function run($items)
{
    $key = key($items);
    $value = current($items);
    next($items);
    $a = key($items);
    $b = current($items);
    next($items);
    $k = key($items);
    $v = current($items);
    next($items);
    list(, ) = each($items);
    list($only) = each($items);
    if ($items) {
        $c = key($items);
        $d = current($items);
        next($items);
    }
    $x = list($e, $f) = each($items);
}
