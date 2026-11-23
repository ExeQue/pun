<?php

namespace App;

use Illuminate\Support\Collection;

function run(Collection $c, $other)
{
    $a = $c->count() <> 0;
    $b = (0) == $c->count();
    $d = $c->count(fn ($x) => $x) > 0;
    $e = $c->COUNT() > 0;
    $f = $c->count() > 0x0;
    $g = $other->count() > 0;
    return ($c->count()) === 0;
}
-----
<?php

namespace App;

use Illuminate\Support\Collection;

function run(Collection $c, $other)
{
    $a = $c->isNotEmpty();
    $b = $c->isEmpty();
    $d = $c->count(fn ($x) => $x) > 0;
    $e = $c->isNotEmpty();
    $f = $c->isNotEmpty();
    $g = $other->count() > 0;
    return $c->isEmpty();
}
