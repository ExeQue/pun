<?php

$x = mktime();
$y = mktime(1);
$z = MKTIME();
$w = \mktime();
$v = mktime(...);
-----
<?php

$x = time();
$y = mktime(1);
$z = time();
$w = time();
$v = mktime(...);
