<?php

namespace App\Tests;

use PHPUnit\Framework\Assert;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe(): void
    {
        Assert::assertSame(
            1,
            2
        );
        \PHPUnit\Framework\Assert::assertTrue(true);
        $fn = function () {
            Assert::assertNull(null);
        };
        $static = static function () {
            Assert::assertNull(null);
        };
        $anon = new class {
            public function run(): void
            {
                Assert::assertTrue(true);
            }
        };
        Assert::fail('x');
    }

    public static function provider(): void
    {
        Assert::assertTrue(true);
    }
}

final class Plain
{
    public function run(): void
    {
        Assert::assertTrue(true);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Assert;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe(): void
    {
        $this->assertSame(1, 2);
        $this->assertTrue(true);
        $fn = function () {
            $this->assertNull(null);
        };
        $static = static function () {
            Assert::assertNull(null);
        };
        $anon = new class {
            public function run(): void
            {
                Assert::assertTrue(true);
            }
        };
        $this->fail('x');
    }

    public static function provider(): void
    {
        Assert::assertTrue(true);
    }
}

final class Plain
{
    public function run(): void
    {
        Assert::assertTrue(true);
    }
}
