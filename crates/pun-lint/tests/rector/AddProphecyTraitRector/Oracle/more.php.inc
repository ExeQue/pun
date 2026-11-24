<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;
use Prophecy\PhpUnit\ProphecyTrait;

final class ProphecyStaticTest extends TestCase
{
    use \Some\OtherTrait;

    private $a;

    public function test()
    {
        $make = function () {
            return self::prophesize(\Foo::class);
        };
    }
}

final class ProphecyImportedTest extends TestCase
{
    use ProphecyTrait;

    public function test()
    {
        $this->prophesize(\Foo::class);
    }
}

final class ProphecyOtherReceiverTest extends TestCase
{
    public function test($other)
    {
        $other->prophesize(\Foo::class);
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;
use Prophecy\PhpUnit\ProphecyTrait;

final class ProphecyStaticTest extends TestCase
{
    use \Prophecy\PhpUnit\ProphecyTrait;
    use \Some\OtherTrait;

    private $a;

    public function test()
    {
        $make = function () {
            return self::prophesize(\Foo::class);
        };
    }
}

final class ProphecyImportedTest extends TestCase
{
    use ProphecyTrait;

    public function test()
    {
        $this->prophesize(\Foo::class);
    }
}

final class ProphecyOtherReceiverTest extends TestCase
{
    public function test($other)
    {
        $other->prophesize(\Foo::class);
    }
}
