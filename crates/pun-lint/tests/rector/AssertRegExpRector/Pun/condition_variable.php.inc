<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class RegExpConditionTest extends TestCase
{
    public function test(string $string, int $expected)
    {
        $this->assertSame($expected, preg_match('/a/', $string));
        $this->assertSame(1, preg_match('/a/'));
    }
}
