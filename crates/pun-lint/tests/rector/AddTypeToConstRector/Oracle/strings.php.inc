<?php

namespace App\Strings;

final class Interpolated
{
    public const DOCUMENT = <<<TXT
        a {$x}
        TXT;
    public const DOUBLE = "a$x";
}
