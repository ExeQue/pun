<?php

namespace App;

class Search
{
    public function all(array $items): bool
    {
        $ok = array_all($items, fn($item) => $item > 10);
        return $ok;
    }

    public function any(array $items): bool
    {
        $found = false;
        foreach ($items as $item) {
            if ($item > 10) {
                $found = true;
                break;
            }
        }
        return $found;
    }

    public function find(array $items): mixed
    {
        $found = null;
        foreach ($items as $item) {
            if ($item > 10) {
                $found = $item;
                break;
            }
        }
        return $found;
    }

    public function findKey(array $items): mixed
    {
        $found = array_find_key($items, fn($item) => $item > 10);
        return $found;
    }
}
