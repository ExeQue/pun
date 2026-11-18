<?php

namespace App;

final class EmptyArrays
{
    /** @var array */
    public $property = [];

    public function isArray($values): bool
    {
        return $values === [];
    }

    public function empty(): bool
    {
        return $this->property === [];
    }
}
