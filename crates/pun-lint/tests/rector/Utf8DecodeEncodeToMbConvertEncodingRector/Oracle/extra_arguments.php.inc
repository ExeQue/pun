<?php

$a = utf8_decode($x, $y);
$b = utf8_encode($x, $y);
$c = utf8_encode($x, $y, $z);
-----
<?php

$a = mb_convert_encoding($x, 'ISO-8859-1');
$b = mb_convert_encoding($x, 'UTF-8', 'ISO-8859-1');
$c = mb_convert_encoding($x, 'UTF-8', 'ISO-8859-1');
