<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreLayoutTest extends TestCase
{
    public function testMergeEmpty()
    {
        $matcher = $this->exactly(2);
        $this->mock->expects($matcher)->method('a')->withConsecutive([1], [2])->willReturnCallback(function ($first) use ($matcher) {});
    }

    public function testReference()
    {
        $this->mock->method('a')
            ->willReturnReference($ref)
            ->withConsecutive(['a' => 1, ...$rest], array(fn ($x) => $x > 1, $this->callback(fn ($y) => $y > 2)));
    }

    public function testUnpackStatic()
    {
        self::$mock->expects(self::atLeastOnce())->method('a')->withConsecutive(...$this->cases());
    }

    public function testAssigned()
    {
        $builder = $this->mock->method('a')->withConsecutive([1], [2]);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreLayoutTest extends TestCase
{
    public function testMergeEmpty()
    {
        $matcher = $this->exactly(2);
        $this->mock->expects($matcher)->expects($matcher)->method('a')->willReturnCallback(function ($first, $parameters) use ($matcher) {
            if ($matcher->numberOfInvocations() === 1) {
                $this->assertSame(1, $parameters[0]);
            }
            if ($matcher->numberOfInvocations() === 2) {
                $this->assertSame(2, $parameters[0]);
            }
        });
    }

    public function testReference()
    {
        $matcher = $this->exactly(2);
        $this->mock->expects($matcher)->method('a')->willReturnCallback(function &(...$parameters) use ($matcher, $rest, $x, $y, $ref) {
            if ($matcher->numberOfInvocations() === 1) {
                $this->assertSame(1, $parameters[0]);
                $this->assertSame($rest, $parameters[1]);
            }
            if ($matcher->numberOfInvocations() === 2) {
                $this->assertSame(fn ($x) => $x > 1, $parameters[0]);
                $callback = fn ($y) => $y > 2;
                $this->assertTrue($callback($parameters[1]));
            }
            return $ref;
        });
    }

    public function testUnpackStatic()
    {
        $matcher = self::atLeastOnce();
        self::$mock->expects($matcher)->method('a')->willReturnCallback(function (...$parameters) use ($matcher) {
            $this->assertSame($this->cases()[$matcher->numberOfInvocations() - 1], $parameters);
        });
    }

    public function testAssigned()
    {
        $builder = $this->mock->method('a')->withConsecutive([1], [2]);
    }
}
