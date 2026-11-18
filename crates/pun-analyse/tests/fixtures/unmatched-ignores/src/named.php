<?php

namespace App;

class Named
{
    public function run(int $value): void
    {
    }
}

function named(Named $named): void
{
    $named->run(value: 1);
}
