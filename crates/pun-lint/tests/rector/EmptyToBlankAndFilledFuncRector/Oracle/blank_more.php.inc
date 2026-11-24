<?php

namespace App;

function run($a, $b)
{
    if (empty($a['x'])) {
        return 1;
    }
    $c = !empty($b) && empty($a);
    $d = ! (empty($a));
    $e = !!empty($b);
    return empty($a->prop) ? 1 : 2;
}
-----
<?php

namespace App;

function run($a, $b)
{
    if (blank($a['x'])) {
        return 1;
    }
    $c = filled($b) && blank($a);
    $d = filled($a);
    $e = !filled($b);
    return blank($a->prop) ? 1 : 2;
}
