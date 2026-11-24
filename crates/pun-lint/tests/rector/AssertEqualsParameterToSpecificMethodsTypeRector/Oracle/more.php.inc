<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class EqualsMoreTest extends TestCase
{
    public function test($value, $delta, $message)
    {
        $this->assertEquals('a', $value, 'message', 0.0, 10, false, true);
        self::assertNotEquals('a', $value, $message, 0.0, 10, true);
        $this->assertEquals('a', $value, '', 0.0, 10, true);
        $this->assertEquals('a', $value, 'message', 0.0, 10, false);
        $this->assertEquals('a', $value, 'message', 0);
        $this->assertNotEquals('a', $value, 'message', $delta);
        $this->assertEquals('a', $value, '', 0.0);
        $this->assertEquals('a', $value, 'message', 1.5, 3);
        $this->assertEquals('a', $value, 'message');
        $this->assertSame('a', $value, 'message', 1.5);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class EqualsMoreTest extends TestCase
{
    public function test($value, $delta, $message)
    {
        $this->assertEqualsIgnoringCase('a', $value, 'message');
        self::assertEqualsCanonicalizing('a', $value, $message);
        $this->assertEqualsCanonicalizing('a', $value);
        $this->assertEquals('a', $value, 'message');
        $this->assertEqualsWithDelta('a', $value, 0, 'message');
        $this->assertEqualsWithDelta('a', $value, $delta, 'message');
        $this->assertEquals('a', $value, '');
        $this->assertEqualsWithDelta('a', $value, 1.5, 'message');
        $this->assertEquals('a', $value, 'message');
        $this->assertSame('a', $value, 'message', 1.5);
    }
}
