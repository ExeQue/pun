<?php

function run(string $text, int $number, $mixed)
{
    [$a, $b] = $text;
    list($c) = $number;
    list($d) = $mixed;
    [$e] = 'abc' . $text;
    [$f] = ($text);
    list($g) = str_split($text);
}
-----
<?php

function run(string $text, int $number, $mixed)
{
    [$a, $b] = str_split($text);
    list($c) = $number;
    list($d) = $mixed;
    [$e] = str_split('abc' . $text);
    [$f] = (str_split($text));
    list($g) = str_split($text);
}
