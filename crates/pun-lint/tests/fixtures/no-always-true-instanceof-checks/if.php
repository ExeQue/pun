<?php

namespace App;

use stdClass;

final class Instanceofs
{
    public function assert(stdClass $var): void
    {
        assert($var instanceof stdClass);
        echo 'assert';
    }

    public function if(stdClass $var): void
    {
        echo 'if';
    }
}
