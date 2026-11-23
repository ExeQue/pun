<?php

namespace App;

abstract class Platform
{
    public function fold(string $c): string
    {
        return $c;
    }
}

class Pg extends Platform {}

final class Sqlite {}

function f(Platform $p, string $c): string
{
    if (method_exists($p, 'fold')) {
        return $p->fold($c);
    }

    \PHPStan\dumpType($p);
    if ($p instanceof Pg) {
        return strtolower($c);
    }

    return $c;
}

function g(Platform|Sqlite $p): string
{
    if (! method_exists($p, 'fold')) {
        \PHPStan\dumpType($p);

        return 'none';
    }

    \PHPStan\dumpType($p);

    return 'some';
}
