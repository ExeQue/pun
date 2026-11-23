<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class FallbackTest extends TestCase
{
    public function test(): void
    {
        $mock = $this->createMock(\stdClass::class);
        $mock->method('a')->willReturnCallback(fn ($x) => true);
        $mock->method('b')->willReturnCallback(function ($x) {
        });
        $mock->method('c')->willReturnCallback(function ($x) {
            if ($x) {
                return \true;
            }
            $f = function () {
                return 1;
            };
            ;
        });
        $mock->method('d')->willReturnCallback(function ($x) {
            if ($x) {
                return true;
            }
            return;
        }, 1);
        $mock->method('e')->WILLRETURNCALLBACK(function ($x) {
            if ($x) {
                return false;
            }
            $this->markTestSkipped('no');
        });
        $mock->method('f')->willReturnCallback(function ($x) {
            if ($x) {
                return null;
            }
        });
    }
}

final class NotATest
{
    public function run($mock): void
    {
        $mock->willReturnCallback(function ($x) {
            if ($x) {
                return true;
            }
        });
    }
}
-----
<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class FallbackTest extends TestCase
{
    public function test(): void
    {
        $mock = $this->createMock(\stdClass::class);
        $mock->method('a')->willReturnCallback(fn ($x) => true);
        $mock->method('b')->willReturnCallback(function ($x) {
        });
        $mock->method('c')->willReturnCallback(function ($x) {
            if ($x) {
                return \true;
            }
            $f = function () {
                return 1;
            };
            return false;
            ;
        });
        $mock->method('d')->willReturnCallback(function ($x) {
            if ($x) {
                return true;
            }
            return;
        }, 1);
        $mock->method('e')->WILLRETURNCALLBACK(function ($x) {
            if ($x) {
                return false;
            }
            $this->markTestSkipped('no');
        });
        $mock->method('f')->willReturnCallback(function ($x) {
            if ($x) {
                return null;
            }
        });
    }
}

final class NotATest
{
    public function run($mock): void
    {
        $mock->willReturnCallback(function ($x) {
            if ($x) {
                return true;
            }
        });
    }
}
