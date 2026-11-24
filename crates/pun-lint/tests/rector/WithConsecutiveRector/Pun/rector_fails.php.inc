<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class RectorFailsTest extends TestCase
{
    public function testNotImplemented()
    {
        $this->mock->method('a')->withConsecutive($this->callback(fn ($x) => $x > 1));
    }

    public function testNoArgument()
    {
        $this->mock->method('a')->withConsecutive([1])->willReturn();
    }
}
