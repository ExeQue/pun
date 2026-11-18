<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class ParamsTest extends TestCase
{
    public function testRun()
    {
        return $this->create(1000) . $this->run2('a');
    }

    private function create($value)
    {
        $value = 500;
        return $value;
    }

    private function run2(string $used)
    {
        return $used;
    }
}
