<?php

function run($haystack, $needle)
{
    $a = strncmp($haystack, $needle, strlen($needle)) === 1;
    $b = strncmp($haystack, 'abc', 3) <> 0;
    $c = !(substr($haystack, 0, strlen($needle)) === $needle);
    $d = substr($haystack, 0, 3) === 'abc' && $c;
    $e = 0 === \strpos($haystack, $needle);
    $f = strpos($haystack, $needle, 0) !== 0;
    $g = substr($haystack, 0, 4) === 'abc';
    $h = strpos($haystack, $needle) === '0';
    $i = SUBSTR($haystack, 0, strlen($needle)) == $needle;
    $j = strpos($haystack, $needle) < 0;
    $k = substr($haystack, 0, strlen($needle)) === $needle . 'x';
}
-----
<?php

function run($haystack, $needle)
{
    $a = str_starts_with($haystack, $needle);
    $b = !str_starts_with($haystack, 'abc');
    $c = !(str_starts_with($haystack, $needle));
    $d = str_starts_with($haystack, 'abc') && $c;
    $e = str_starts_with($haystack, $needle);
    $f = !str_starts_with($haystack, $needle);
    $g = substr($haystack, 0, 4) === 'abc';
    $h = strpos($haystack, $needle) === '0';
    $i = str_starts_with($haystack, $needle);
    $j = strpos($haystack, $needle) < 0;
    $k = substr($haystack, 0, strlen($needle)) === $needle . 'x';
}
