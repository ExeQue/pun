<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class ImportedDataProviderTest extends TestCase
{
    /**
     * @dataProvider testData
     */
    public function testOne($a)
    {
    }

    public function testData(): array
    {
        return [[1]];
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class ImportedDataProviderTest extends TestCase
{
    /**
     * @dataProvider data
     */
    public function testOne($a)
    {
    }

    public function data(): array
    {
        return [[1]];
    }
}
