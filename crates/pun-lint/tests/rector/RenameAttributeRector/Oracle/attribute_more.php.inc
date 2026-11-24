<?php

namespace App;

use Lib\OldAttr;

#[OldAttr]
class More
{
    #[OldAttr(1), \Lib\Other]
    public $p;

    #[\lib\oldattr]
    public function run(#[OldAttr] $x)
    {
        $f = #[OldAttr] fn () => 1;
    }

    #[OldAttr]
    public const C = 1;
}

#[OldAttr]
function f()
{
}

#[OldAttr]
enum E
{
}
-----
<?php

namespace App;

use Lib\OldAttr;

#[\Lib\NewAttr]
class More
{
    #[\Lib\NewAttr(1), \Lib\Other]
    public $p;

    #[\Lib\NewAttr]
    public function run(#[\Lib\NewAttr] $x)
    {
        $f = #[OldAttr] fn () => 1;
    }

    #[OldAttr]
    public const C = 1;
}

#[OldAttr]
function f()
{
}

#[OldAttr]
enum E
{
}
