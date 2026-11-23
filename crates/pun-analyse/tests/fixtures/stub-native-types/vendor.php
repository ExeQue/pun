<?php

namespace Lib;

class Factory
{
    /**
     * @param  int|null|iterable<int, array<string, mixed>>  $records
     * @return list<string>
     */
    public function createMany(int|iterable|null $records = null)
    {
        return [];
    }

    public function name(): string|int
    {
        return '';
    }
}
