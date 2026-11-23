<?php

namespace App;

class Schema
{
    /** @return array<string, int> */
    public function plain(array $parts): array
    {
        return [...$parts];
    }

    /**
     * @param array<array-key, int> $parts
     * @return array<string, int>
     */
    public function arrayKeys(array $parts): array
    {
        return [...$parts, ...$parts];
    }

    /**
     * @param array<int, int> $parts
     * @return array<string, int>
     */
    public function intKeys(array $parts): array
    {
        return [...$parts];
    }

    /**
     * @param array<string, int> $parts
     * @return list<int>
     */
    public function stringKeys(array $parts): array
    {
        return [...$parts];
    }
}
