<?php

namespace App;

const LOCAL = 'x';

class K
{
    public const A = 3;
}

function f(string $s): int
{
    \PHPStan\dumpType(constant('CURLOPT_SHARE'));
    \PHPStan\dumpType(constant('PHP_INT_SIZE'));
    \PHPStan\dumpType(constant('App\LOCAL'));
    \PHPStan\dumpType(constant('App\K::A'));
    \PHPStan\dumpType(constant('\App\K::A'));
    \PHPStan\dumpType(constant($s));

    return (int) constant('CURLOPT_SHARE') + (int) constant($s);
}
