<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class ParamsTest extends TestCase
{
    public function testRun()
    {
        return $this->create() . $this->run2('a', 1);
    }

    private function create()
    {
        $value = 500;
        return $value;
    }

    private function run2(string $used, int $notUsed)
    {
        return $used;
    }
}
