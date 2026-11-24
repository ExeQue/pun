<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

// Rector inserts another assertArrayHasKey() on every run for a key that is not a string literal; Pun leaves it alone.
final class NonStringKeyTest extends TestCase
{
    public function testKeys(): void
    {
        $data = $this->data();
        $this->assertSame(3, $data[0]);
        $this->assertSame(4, $data[$key]);
    }
}
