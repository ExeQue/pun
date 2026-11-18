<?php

namespace App;

final class InArray
{
    public function repeatedOr($someVariable): bool
    {
        return $someVariable === 'a' || $someVariable === 'b' || $someVariable === 'c';
    }

    public function foreach(array $items)
    {
        foreach ($items as $item) {
            if ($item == 'something') {
                return true;
            }
        }

        return false;
    }

    public function arraySearch(array $array): bool
    {
        return in_array('searching', $array, true);
    }
}
