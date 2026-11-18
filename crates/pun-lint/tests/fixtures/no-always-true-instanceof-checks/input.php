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
        if ($var instanceof stdClass) {
            echo 'if';
        }
    }
}
