<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function test_this_is_it(): void
    {
    }

    public function test__double(): void
    {
    }

    /**
     * @test
     */
    public function it_works(): void
    {
    }

    #[Test]
    public function by_attribute(): void
    {
    }

    protected function test_protected(): void
    {
    }

    public function helper_method(): void
    {
    }

    public function test_a(): void
    {
    }

    public function testA(): void
    {
    }
}

final class NotATest
{
    public function test_me(): void
    {
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

final class MoreTest extends TestCase
{
    public function testThisIsIt(): void
    {
    }

    public function testDouble(): void
    {
    }

    /**
     * @test
     */
    public function itWorks(): void
    {
    }

    #[Test]
    public function byAttribute(): void
    {
    }

    protected function test_protected(): void
    {
    }

    public function helper_method(): void
    {
    }

    public function test_a(): void
    {
    }

    public function testA(): void
    {
    }
}

final class NotATest
{
    public function test_me(): void
    {
    }
}
