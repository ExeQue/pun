<?php

namespace App;

function run(array $rest, $x)
{
    $a = date('Y', ...$rest);
    $l = date(timestamp: 5, format: 'Y');
    $n = date('Y', strtotime(...$rest));
    $p = date('Y', strtotime('now', 5));
    $v = time() - 60 * $x;
}
-----
<?php

namespace App;

function run(array $rest, $x)
{
    $a = date('Y', ...$rest);
    $l = date(timestamp: 5, format: 'Y');
    $n = \Carbon\Carbon::createFromTimestamp(strtotime(...$rest))->format('Y');
    $p = \Carbon\Carbon::createFromTimestamp(strtotime('now', 5))->format('Y');
    $v = time() - 60 * $x;
}
