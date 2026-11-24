<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PublicProviderMoreTest extends TestCase
{
    /**
     * @dataProvider provideOne
     */
    public function testOne($a)
    {
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('provideTwo')]
    public function testTwo($a)
    {
    }

    /**
     * @dataProvider provideThree()
     */
    public function testThree($a)
    {
    }

    private static function provideOne(): array
    {
        return [[1]];
    }

    final protected function provideTwo(): array
    {
        return [[2]];
    }

    protected static function provideThree(): iterable
    {
        yield [3];
    }

    protected function notAProvider()
    {
    }
}

final class PublicProviderNotTest
{
    /**
     * @dataProvider provide
     */
    public function testOne($a)
    {
    }

    protected function provide()
    {
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PublicProviderMoreTest extends TestCase
{
    /**
     * @dataProvider provideOne
     */
    public function testOne($a)
    {
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('provideTwo')]
    public function testTwo($a)
    {
    }

    /**
     * @dataProvider provideThree()
     */
    public function testThree($a)
    {
    }

    public static function provideOne(): array
    {
        return [[1]];
    }

    final public function provideTwo(): array
    {
        return [[2]];
    }

    public static function provideThree(): iterable
    {
        yield [3];
    }

    protected function notAProvider()
    {
    }
}

final class PublicProviderNotTest
{
    /**
     * @dataProvider provide
     */
    public function testOne($a)
    {
    }

    protected function provide()
    {
    }
}
