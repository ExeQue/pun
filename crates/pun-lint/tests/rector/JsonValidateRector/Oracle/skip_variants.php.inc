<?php

namespace App;

$a = json_decode($json, true, 512, JSON_INVALID_UTF8_IGNORE) !== null && json_last_error() === JSON_ERROR_NONE;
$b = json_decode($json, true, $depth) !== null && json_last_error() === JSON_ERROR_NONE;
$c = json_decode($json, true, 2147483648) !== null && json_last_error() === JSON_ERROR_NONE;
$d = json_decode($json) === null && json_last_error() === JSON_ERROR_NONE;
$e = json_decode($json) !== null && json_last_error() !== JSON_ERROR_NONE;
$f = json_decode($json) !== false && json_last_error() === JSON_ERROR_NONE;
$g = json_decode($json) !== null || json_last_error() === JSON_ERROR_NONE;
$h = $valid && json_last_error() === JSON_ERROR_NONE;
$i = json_decode($json) !== null && $valid;
$j = json_decode($json) !== null && json_last_error() === JSON_ERROR_SYNTAX;
$k = json_decode() !== null && json_last_error() === JSON_ERROR_NONE;
$l = json_encode($json) !== null && json_last_error() === JSON_ERROR_NONE;
