<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

/**
 * @method void magicOne()
 * @method int magicTwo()
 */
class Magic
{
    public function real()
    {
    }
}

final class SetMethodsMoreTest extends TestCase
{
    public function test()
    {
        $builder = $this->getMockBuilder(Magic::class);
        $builder->setMethods(['real']);
        $mock = $this->getMockBuilder(Magic::class)
            ->disableOriginalConstructor()
            ->setMethods(['magicTwo', 'real', 'magicOne'])
            ->getMock();
        $other = $this->getMockBuilder('App\Tests\Magic')->setMethods(null)->getMock();
        $none = $this->getMockBuilder(\stdClass::class)->setMethods(['missing'])->getMock();
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

/**
 * @method void magicOne()
 * @method int magicTwo()
 */
class Magic
{
    public function real()
    {
    }
}

final class SetMethodsMoreTest extends TestCase
{
    public function test()
    {
        $builder = $this->getMockBuilder(Magic::class);
        $builder;
        $mock = $this->getMockBuilder(Magic::class)
            ->disableOriginalConstructor()
            ->setMethods(['magicTwo', 'real', 'magicOne'])
            ->getMock();
        $other = $this->getMockBuilder('App\Tests\Magic')->getMock();
        $none = $this->getMockBuilder(\stdClass::class)->getMock();
    }
}
