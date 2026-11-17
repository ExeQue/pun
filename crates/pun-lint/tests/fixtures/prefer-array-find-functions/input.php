<?php

namespace App;

class Search
{
    public function all(array $items): bool
    {
        $ok = true;
        foreach ($items as $item) {
            if (!($item > 10)) {
                $ok = false;
                break;
            }
        }
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
        $found = null;
        foreach ($items as $key => $item) {
            if ($item > 10) {
                $found = $key;
                break;
            }
        }
        return $found;
    }
}
