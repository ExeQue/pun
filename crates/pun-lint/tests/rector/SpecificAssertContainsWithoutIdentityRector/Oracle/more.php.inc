<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ContainsEqualsMoreTest extends TestCase
{
    public function test(array $objects, ?string $nullable, string $string, $flag)
    {
        self::assertContains(new \stdClass(), $objects, 'message', false, false);
        $this->assertNotContains(new \stdClass(), $objects, 'message', true, $flag);
        $this->assertContains(new \stdClass(), $objects, 'message', false, false, true);
        $this->assertContains('a', $nullable, 'message', false, false);
        $this->assertContains('a', $string, 'message', false, false);
        $this->assertContains(new \stdClass(), $objects, 'message', false, true);
        $this->assertContains(new \stdClass(), $objects, 'message', false, TRUE);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ContainsEqualsMoreTest extends TestCase
{
    public function test(array $objects, ?string $nullable, string $string, $flag)
    {
        self::assertContainsEquals(new \stdClass(), $objects, 'message');
        $this->assertNotContainsEquals(new \stdClass(), $objects, 'message');
        $this->assertContainsEquals(new \stdClass(), $objects, 'message', true);
        $this->assertContainsEquals('a', $nullable, 'message');
        $this->assertContains('a', $string, 'message', false, false);
        $this->assertContains(new \stdClass(), $objects, 'message', false, true);
        $this->assertContains(new \stdClass(), $objects, 'message', false, TRUE);
    }
}
