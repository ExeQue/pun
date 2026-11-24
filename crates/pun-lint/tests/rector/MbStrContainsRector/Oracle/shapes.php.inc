<?php

function run($haystack, $needle, int $offset)
{
    $a = !(mb_strpos($haystack, $needle) === false);
    $b = false <> MB_STRSTR($haystack, $needle, true);
    $c = mb_strpos($haystack, $needle, 0, 'UTF-8') !== false;
    $d = mb_strpos($haystack, $needle, $offset) !== false;
    $e = \mb_strpos($haystack, $needle) == false && $a;
    $f = mb_stripos($haystack, $needle) !== false;
    $g = mb_strpos($haystack, $needle) !== null;
    $h = false !== strpos($haystack, $needle);
}
-----
<?php

function run($haystack, $needle, int $offset)
{
    $a = !(!str_contains($haystack, $needle));
    $b = str_contains($haystack, $needle);
    $c = str_contains($haystack, $needle, 'UTF-8');
    $d = mb_strpos($haystack, $needle, $offset) !== false;
    $e = !str_contains($haystack, $needle) && $a;
    $f = mb_stripos($haystack, $needle) !== false;
    $g = mb_strpos($haystack, $needle) !== null;
    $h = false !== strpos($haystack, $needle);
}
