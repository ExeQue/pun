<?php

namespace App;

use Lib\Factory;

function f(Factory $factory): void
{
    $factory->createMany(5);
    $factory->createMany();
    $factory->createMany('x');
    $factory->createMany([['a' => 1]]);
}

function g(Factory $factory): int
{
    return $factory->name();
}
