<?php

function run($items)
{
    while ([$key, $value] = each($items)) {
        echo $key;
    }
    while (list($k, $v) = each($items)) echo $v;
    while (list($k, $v) = each($items)):
        echo $v;
    endwhile;
    while (list($k, $v, $w) = each($items)) {
    }
    while ($pair = each($items)) {
    }
}
-----
<?php

function run($items)
{
    foreach ($items as $key => $value) {
        echo $key;
    }
    foreach ($items as $k => $v) {
        echo $v;
    }
    foreach ($items as $k => $v) {
        echo $v;
    }
    foreach ($items as $v => $w) {
    }
    while ($pair = each($items)) {
    }
}
