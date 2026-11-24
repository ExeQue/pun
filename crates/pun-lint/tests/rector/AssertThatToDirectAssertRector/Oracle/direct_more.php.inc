<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe($value): void
    {
        $this->assertThat($value, $this->isTrue());
        $this->assertThat($value, $this->isNull(), 'message');
        $this->assertThat($value, $this->equalTo(5));
        $this->assertThat($value, $this->isType('string'));
        $this->assertThat($value, $this->isType('int'));
        $this->assertThat($value, $this->isType($type));
        $this->assertThat($value, $this->ISFALSE());
        $this->assertThat($value, self::isTrue());
        self::assertThat($value, $this->isArray());
        $this->assertThat($value, $this->logicalNot($this->isTrue()));
        $this->assertThat(
            $value,
            $this->isEmpty()
        );
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testMe($value): void
    {
        $this->assertTrue($value);
        $this->assertNull($value, 'message');
        $this->assertEquals(5, $value);
        $this->assertIsString($value);
        $this->assertIsInt($value);
        $this->assertThat($value, $this->isType($type));
        $this->assertFalse($value);
        $this->assertThat($value, self::isTrue());
        self::assertThat($value, $this->isArray());
        $this->assertThat($value, $this->logicalNot($this->isTrue()));
        $this->assertEmpty(
            $value
        );
    }
}
