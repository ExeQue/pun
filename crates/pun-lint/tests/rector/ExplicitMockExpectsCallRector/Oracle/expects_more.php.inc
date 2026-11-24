<?php

namespace App\Tests;

use PHPUnit\Framework\MockObject\MockObject;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testChain(): void
    {
        $mock = $this->createMock(\stdClass::class);
        $mock->method('a')->willReturn(1);
        $mock->METHOD('b');
        $fn = function () use ($mock) {
            $mock->method('c');
        };
        $stub = $this->createStub(\stdClass::class);
        $stub->method('d');
    }

    public function testParam(MockObject $mock, ?MockObject $nullable): void
    {
        $mock->method('e');
        $nullable->method('f');
    }

    protected function testProtected(MockObject $mock): void
    {
        $mock->method('g');
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\MockObject\MockObject;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testChain(): void
    {
        $mock = $this->createMock(\stdClass::class);
        $mock->expects($this->atLeastOnce())->method('a')->willReturn(1);
        $mock->expects($this->atLeastOnce())->METHOD('b');
        $fn = function () use ($mock) {
            $mock->expects($this->atLeastOnce())->method('c');
        };
        $stub = $this->createStub(\stdClass::class);
        $stub->method('d');
    }

    public function testParam(MockObject $mock, ?MockObject $nullable): void
    {
        $mock->expects($this->atLeastOnce())->method('e');
        $nullable->expects($this->atLeastOnce())->method('f');
    }

    protected function testProtected(MockObject $mock): void
    {
        $mock->method('g');
    }
}
