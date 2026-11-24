<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

final class SnakeTest extends TestCase
{
    public function testHTTPRequest(): void
    {
    }

    public function test_Foo(): void
    {
    }

    public function testlower(): void
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

    private function testPrivate(): void
    {
    }

    public function test1Two(): void
    {
    }

    public function testB(): void
    {
    }

    public function TEST_b(): void
    {
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;
use PHPUnit\Framework\TestCase;

final class SnakeTest extends TestCase
{
    public function test_h_t_t_p_request(): void
    {
    }

    public function test__foo(): void
    {
    }

    public function testlower(): void
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

    private function testPrivate(): void
    {
    }

    public function test1_two(): void
    {
    }

    public function testB(): void
    {
    }

    public function TEST_b(): void
    {
    }
}
