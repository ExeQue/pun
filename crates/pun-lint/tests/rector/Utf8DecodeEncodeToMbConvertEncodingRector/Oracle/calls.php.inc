<?php

namespace App;

$a = utf8_decode($x);
$b = \utf8_encode($x);
$c = UTF8_DECODE($x,);
$d = utf8_encode(
    $x
);
$e = utf8_decode(string: $x);
$f = utf8_encode(...$args);
$g = utf8_decode(...);
$h = utf8_encode();
$i = $o->utf8_decode($x);
-----
<?php

namespace App;

$a = mb_convert_encoding($x, 'ISO-8859-1');
$b = mb_convert_encoding($x, 'UTF-8', 'ISO-8859-1');
$c = mb_convert_encoding($x, 'ISO-8859-1',);
$d = mb_convert_encoding(
    $x, 'UTF-8', 'ISO-8859-1'
);
$e = mb_convert_encoding(string: $x, 'ISO-8859-1');
$f = mb_convert_encoding(...$args, 'UTF-8', 'ISO-8859-1');
$g = utf8_decode(...);
$h = mb_convert_encoding('UTF-8', 'ISO-8859-1');
$i = $o->utf8_decode($x);
