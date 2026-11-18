<?php

namespace App;

final class EmptyArrays
{
    /** @var array */
    public $property = [];

    public function isArray($values): bool
    {
        return empty($values) && is_array($values);
    }

    public function empty(): bool
    {
        return empty($this->property);
    }
}
