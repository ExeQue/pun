<?php

function run(string $haystack, int $int, $mixed, string $string, ?string $maybe)
{
    strpos($haystack, $int);
    strrpos($haystack, $mixed);
    stristr($haystack, $string);
    strstr($haystack, $maybe);
    strchr($haystack, $int + 1);
    strrchr($haystack, "x{$string}");
    strripos($haystack, needle: $int);
    str_contains($haystack, $int);
    strpos($haystack);
}
-----
<?php

function run(string $haystack, int $int, $mixed, string $string, ?string $maybe)
{
    strpos($haystack, (string) $int);
    strrpos($haystack, (string) $mixed);
    stristr($haystack, $string);
    strstr($haystack, (string) $maybe);
    strchr($haystack, (string) ($int + 1));
    strrchr($haystack, "x{$string}");
    strripos($haystack, needle: (string) $int);
    str_contains($haystack, $int);
    strpos($haystack);
}
