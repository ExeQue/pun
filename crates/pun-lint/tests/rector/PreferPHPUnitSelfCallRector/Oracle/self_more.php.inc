<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe(): void
    {
        $this->assertSame(
            1,
            2
        );
        $this->assertTrue(true)->foo;
        $fn = function () {
            $this->assertNull(null);
        };
        $this->expectException(\RuntimeException::class);
        $this->createMock(\stdClass::class);
        $this->ASSERTTRUE(true);
        $this->markTestSkipped('x');
        $this->assertCustom();
    }

    public function assertCustom(): void
    {
    }
}

final class Plain
{
    public function run(TestCase $case): void
    {
        $case->assertTrue(true);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe(): void
    {
        self::assertSame(1, 2);
        self::assertTrue(true)->foo;
        $fn = function () {
            self::assertNull(null);
        };
        $this->expectException(\RuntimeException::class);
        $this->createMock(\stdClass::class);
        $this->ASSERTTRUE(true);
        $this->markTestSkipped('x');
        $this->assertCustom();
    }

    public function assertCustom(): void
    {
    }
}

final class Plain
{
    public function run(TestCase $case): void
    {
        $case->assertTrue(true);
    }
}
