<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PrefixMoreTest extends TestCase
{
    /**
     * @dataProvider testData
     * @dataProvider testMissing
     */
    public function testOne($a)
    {
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('testAttributed')]
    public function testTwo($a)
    {
    }

    /** @dataProvider testInline */
    public function testThree($a)
    {
    }

    public function testData(): array
    {
        return [[1]];
    }

    public function testAttributed(): array
    {
        return [[2]];
    }

    public function testInline(): array
    {
        return [[3]];
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PrefixMoreTest extends TestCase
{
    /**
     * @dataProvider data
     * @dataProvider missing
     */
    public function testOne($a)
    {
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('testAttributed')]
    public function testTwo($a)
    {
    }

    /** @dataProvider inline */
    public function testThree($a)
    {
    }

    public function data(): array
    {
        return [[1]];
    }

    public function attributed(): array
    {
        return [[2]];
    }

    public function inline(): array
    {
        return [[3]];
    }
}
