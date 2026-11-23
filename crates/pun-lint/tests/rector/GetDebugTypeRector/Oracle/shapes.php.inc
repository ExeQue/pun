<?php

function types($value, $other)
{
    $a = is_object($value) ? get_class($value) : gettype($value);
    $b = (is_object($value)) ? (get_class($value)) : (gettype($value));
    $c = is_object($value) ? get_class($other) : gettype($value);
    $d = is_object($value->x) ? $value->x::class : gettype($value->x);
    $e = is_object($value) ?: gettype($value);
    $f = is_object($value) ? get_class() : gettype($value);
    $g = IS_OBJECT($value) ? \get_class($value) : \GetType($value);
}
-----
<?php

function types($value, $other)
{
    $a = get_debug_type($value);
    $b = get_debug_type($value);
    $c = is_object($value) ? get_class($other) : gettype($value);
    $d = get_debug_type($value->x);
    $e = is_object($value) ?: gettype($value);
    $f = is_object($value) ? get_class() : gettype($value);
    $g = get_debug_type($value);
}
