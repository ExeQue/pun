<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testFirst(): void
    {
        self::assertSame(1, $data['a']);
        $this->assertSame(2, $data['a']);
        $this->assertSame(3, $data["q"]);
        static::assertArrayHasKey('b', $data);
        self::assertSame(5, $data['b']);
        $this->assertSame(6, $other['b']);
        $this->assertEquals(7, $data['c']);
        $this->assertSame(8, $data['x']['y']);
        if (true) {
            $this->assertSame(9, $data['d']);
        }
        $this->assertSame(10, $this->data['e']);
    }

    public function testOne(): void
    {
        $this->assertSame(1, $data['a']);
    }

    private function testPrivate(): void
    {
        $data = [];
        $this->assertSame(1, $data['a']);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testFirst(): void
    {
        $this->assertArrayHasKey('a', $data, 'Existing keys are: ' . implode(', ', array_keys($data)));
        self::assertSame(1, $data['a']);
        $this->assertArrayHasKey('a', $data, 'Existing keys are: ' . implode(', ', array_keys($data)));
        $this->assertSame(2, $data['a']);
        $this->assertArrayHasKey("q", $data, 'Existing keys are: ' . implode(', ', array_keys($data)));
        $this->assertSame(3, $data["q"]);
        static::assertArrayHasKey('b', $data);
        self::assertSame(5, $data['b']);
        $this->assertArrayHasKey('b', $other, 'Existing keys are: ' . implode(', ', array_keys($other)));
        $this->assertSame(6, $other['b']);
        $this->assertEquals(7, $data['c']);
        $this->assertSame(8, $data['x']['y']);
        if (true) {
            $this->assertSame(9, $data['d']);
        }
        $this->assertSame(10, $this->data['e']);
    }

    public function testOne(): void
    {
        $this->assertSame(1, $data['a']);
    }

    private function testPrivate(): void
    {
        $data = [];
        $this->assertSame(1, $data['a']);
    }
}
