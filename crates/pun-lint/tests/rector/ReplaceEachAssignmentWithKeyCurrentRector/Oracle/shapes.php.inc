<?php

function run($items, $object)
{
    $object->pair = each($items);
    $pair = each($object->items);
    [$a, $b] = each($items);
    $c = each();
    if ($items) $d = each($items);
}
-----
<?php

function run($items, $object)
{
    $object->pair[1] = current($items);
    $object->pair['value'] = current($items);
    $object->pair[0] = key($items);
    $object->pair['key'] = key($items);
    next($items);
    $pair[1] = current($object->items);
    $pair['value'] = current($object->items);
    $pair[0] = key($object->items);
    $pair['key'] = key($object->items);
    next($object->items);
    [$a, $b] = each($items);
    $c = each();
    if ($items) {
        $d[1] = current($items);
        $d['value'] = current($items);
        $d[0] = key($items);
        $d['key'] = key($items);
        next($items);
    }
}
