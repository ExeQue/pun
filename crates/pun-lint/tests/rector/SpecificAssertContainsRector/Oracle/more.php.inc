<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ContainsMoreTest extends TestCase
{
    public function test(?string $nullable, array $items, int|string $mixed, string|false $maybe, $unknown)
    {
        $this->assertContains('a', $nullable);
        self::assertNotContains('a', $nullable, 'message');
        $this->assertContains('a', $items);
        $this->assertContains('a', $mixed);
        static::assertContains('a', $maybe);
        $this->assertContains('a', $unknown);
        $this->assertContains('a', null);
        $this->assertContains('a', false);
        $this->assertContains('a', '');
    }
}

final class NotATest
{
    public function test(string $s)
    {
        $this->assertContains('a', $s);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ContainsMoreTest extends TestCase
{
    public function test(?string $nullable, array $items, int|string $mixed, string|false $maybe, $unknown)
    {
        $this->assertStringContainsString('a', $nullable);
        self::assertStringNotContainsString('a', $nullable, 'message');
        $this->assertContains('a', $items);
        $this->assertContains('a', $mixed);
        static::assertStringContainsString('a', $maybe);
        $this->assertContains('a', $unknown);
        $this->assertContains('a', null);
        $this->assertContains('a', false);
        $this->assertContains('a', '');
    }
}

final class NotATest
{
    public function test(string $s)
    {
        $this->assertContains('a', $s);
    }
}
