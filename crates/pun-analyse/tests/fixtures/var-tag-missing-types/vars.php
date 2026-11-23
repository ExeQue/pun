<?php

namespace App;

/** @template T */
class Box {}

/** @template T */
class Bag {}

function make(): mixed { return new Box(); }

function f(): void
{
    /** @var Box|Bag $a */
    $a = make();

    /** @var array $b */
    $b = make();

    /** @var array<int, Box<int>> $c */
    $c = make();

    $fn = function (): void {
        /** @var iterable $d */
        $d = make();
    };
}
