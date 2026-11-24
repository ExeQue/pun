<?php

function run(array $items, array $other)
{
    reset($items);
    $first = key($items);
    end($other);
    $last = key($other);
    echo key($other);
    reset($items);
    echo 1;
    $key = key($items);
    end($items);
    if (key($items) === 'x') {
        return key($items);
    }
    reset($items);
    $k = key($other);
}
-----
<?php

function run(array $items, array $other)
{
    $first = array_key_first($items);
    $last = array_key_last($other);
    echo array_key_last($other);
    reset($items);
    echo 1;
    $key = key($items);
    if (array_key_last($items) === 'x') {
        return key($items);
    }
    reset($items);
    $k = key($other);
}
