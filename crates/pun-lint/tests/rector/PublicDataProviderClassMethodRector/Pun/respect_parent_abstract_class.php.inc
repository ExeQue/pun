<?php

namespace Rector\PHPUnit\Tests\PHPUnit100\Rector\Class_\PublicDataProviderClassMethodRector\Fixture;

use Rector\PHPUnit\Tests\PHPUnit100\Rector\Class_\PublicDataProviderClassMethodRector\Source\AnotherAbstractClass;

final class RespectParentAbstractClass extends AnotherAbstractClass
{
    protected function dataProvider(): array
    {
        return [
            [1, 2],
        ];
    }
}

?>
