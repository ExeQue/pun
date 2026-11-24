<?php

function run($haystack, $needle)
{
    $a = !(substr($haystack, -strlen($needle)) === $needle);
    $b = 'abc' <> substr($haystack, -3);
    $c = substr($haystack, -4) === 'abc';
    $d = substr_compare($haystack, $needle, -strlen($needle), null, false) === 0;
    $e = substr_compare($haystack, 'abc', -3, 3, true) === 0;
    $f = substr_compare($haystack, $needle, -strlen($needle)) == 1;
    $g = 0 !== substr_compare($haystack, $needle, -strlen($needle));
    $h = substr($haystack, strlen($needle)) === $needle;
    $i = substr($haystack, -strlen($needle), 2) === $needle;
}
-----
<?php

function run($haystack, $needle)
{
    $a = !(str_ends_with($haystack, $needle));
    $b = !str_ends_with($haystack, 'abc');
    $c = substr($haystack, -4) === 'abc';
    $d = str_ends_with($haystack, $needle);
    $e = substr_compare($haystack, 'abc', -3, 3, true) === 0;
    $f = substr_compare($haystack, $needle, -strlen($needle)) == 1;
    $g = !str_ends_with($haystack, $needle);
    $h = substr($haystack, strlen($needle)) === $needle;
    $i = str_ends_with($haystack, $needle);
}
