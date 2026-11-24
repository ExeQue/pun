<?php

namespace App;

use Rector\Tests\Transform\Rector\StaticCall\StaticCallToFuncCallRector\Source\SomeOldStaticClass;

class Other
{
    public function run(SomeOldStaticClass $object, ?SomeOldStaticClass $maybe)
    {
        $a = SomeOldStaticClass::RENDER('x');
        $b = SomeOldStaticClass::render('y', ...[]);
        $c = SomeOldStaticClass::render(name: 'z');
        $e = $object::render('w');
        $f = Other::render();
        $g = SomeOldStaticClass::other();
        $h = \Rector\Tests\Transform\Rector\StaticCall\StaticCallToFuncCallRector\Source\SomeOldStaticClass::render(...);
        $i = SomeOldStaticClass::render()->x;
        $j = self::render();
        $k = $maybe::render();
    }
}
-----
<?php

namespace App;

use Rector\Tests\Transform\Rector\StaticCall\StaticCallToFuncCallRector\Source\SomeOldStaticClass;

class Other
{
    public function run(SomeOldStaticClass $object, ?SomeOldStaticClass $maybe)
    {
        $a = \view('x');
        $b = \view('y', ...[]);
        $c = \view(name: 'z');
        $e = \view('w');
        $f = Other::render();
        $g = SomeOldStaticClass::other();
        $h = \view(...);
        $i = \view()->x;
        $j = self::render();
        $k = \view();
    }
}
