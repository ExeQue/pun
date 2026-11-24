<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class LayoutTest extends TestCase
{
    public function testInlineOne()
    {
        $this->mock->expects($this->once())->method('a')->withConsecutive([1, 'x']);
    }

    public function testInlineTwo()
    {
        $this->mock->expects($this->any())->method('a')->withConsecutive([1], [2]);
    }

    public function testTwoWithReturn()
    {
        // leading comment
        $this->mock->method('a')
            ->withConsecutive([1], [$this->equalTo(2), $this->anything()])
            ->willReturnOnConsecutiveCalls('a', 'b');
    }

    public function testCallback()
    {
        $mock = $this->createMock(\stdClass::class);
        $mock->expects($matcher = $this->exactly(1))->method('a')->withConsecutive(
            [$this->callback(function ($x) use ($mock) {
                return $x > 1;
            }), self::equalTo(3), null, []],
        )->willThrowException(new \Exception('x'));
    }

    public function testEmpty()
    {
        $this->mock->method('a')->withConsecutive([], $x)->willReturnArgument(1);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class LayoutTest extends TestCase
{
    public function testInlineOne()
    {
        $matcher = $this->once();
        $this->mock->expects($matcher)->method('a')->willReturnCallback(function (...$parameters) use ($matcher) {
            if ($matcher->numberOfInvocations() === 1) {
                $this->assertSame(1, $parameters[0]);
                $this->assertSame('x', $parameters[1]);
            }
        });
    }

    public function testInlineTwo()
    {
        $matcher = $this->any();
        $this->mock->expects($matcher)->method('a')->willReturnCallback(function (...$parameters) use ($matcher) {
            if ($matcher->numberOfInvocations() === 1) {
                $this->assertSame(1, $parameters[0]);
            }
            if ($matcher->numberOfInvocations() === 2) {
                $this->assertSame(2, $parameters[0]);
            }
        });
    }

    public function testTwoWithReturn()
    {
        $matcher = $this->exactly(2);
        // leading comment
        $this->mock->expects($matcher)->method('a')->willReturnCallback(function (...$parameters) use ($matcher) {
            if ($matcher->numberOfInvocations() === 1) {
                $this->assertSame(1, $parameters[0]);
                return 'a';
            }
            if ($matcher->numberOfInvocations() === 2) {
                $this->assertEquals(2, $parameters[0]);
                $this->assertSame($this->anything(), $parameters[1]);
                return 'b';
            }
        });
    }

    public function testCallback()
    {
        $mock = $this->createMock(\stdClass::class);
        $matcher = $this->exactly(1);
        $mock->expects($matcher)->expects($matcher = $this->exactly(1))->method('a')->willReturnCallback(
            function (...$parameters) use ($matcher) {
                if ($matcher->numberOfInvocations() === 1) {
                    $callback = function ($x) use ($mock) {
                        return $x > 1;
                    };
                    $this->assertTrue($callback($parameters[0]));
                    $this->assertSame(self::equalTo(3), $parameters[1]);
                    $this->assertSame(null, $parameters[2]);
                    $this->assertSame([], $parameters[3]);
                }
                throw new \Exception('x');
            },
        );
    }

    public function testEmpty()
    {
        $matcher = $this->exactly(2);
        $this->mock->expects($matcher)->method('a')->willReturnCallback(function (...$parameters) use ($matcher, $x) {
            if ($matcher->numberOfInvocations() === 1) {
            }
            if ($matcher->numberOfInvocations() === 2) {
            }
            return $parameters[1];
        });
    }
}
