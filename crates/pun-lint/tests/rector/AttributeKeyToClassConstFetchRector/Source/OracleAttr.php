<?php

namespace App\Attr;

#[\Attribute]
class Column
{
    public function __construct(public $type = null, public $name = null) {}
}

class Types
{
    public const STRING = 'string';
    public const TEXT = 'text';
    public const FIVE = 5;
}
