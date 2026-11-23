<?php

namespace App;

function options(): array
{
    $options['synchronous'] = true;
    $options['allow_redirects'] = false;
    $nested['a']['b'] = 1;

    return [$options, $nested];
}

function unknown(mixed $value): void
{
    $value['x'] = 1;
}
