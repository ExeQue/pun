<?php

namespace App;

final class InArray
{
    public function repeatedOr($someVariable): bool
    {
        return in_array($someVariable, ['a', 'b', 'c'], true);
    }

    public function foreach(array $items)
    {
        return in_array('something', $items);
    }

    public function arraySearch(array $array): bool
    {
        return array_search('searching', $array, true) !== false;
    }
}
