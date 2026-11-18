<?php

namespace App;

trait Shared
{
    public readonly int $s;
}

trait Left
{
    use Shared;
}

trait Right
{
    use Shared;
}

trait Diamond
{
    use Left, Right;
}

final class UsesDiamond
{
    use Diamond;

    public function __construct(public readonly int $a, public readonly int &$b)
    {
    }
}

abstract class Base
{
}

readonly class ReadonlyBase extends Base
{
}

final class NotAllParents extends ReadonlyBase
{
    public readonly int $a;
}

class Open extends ReadonlyBase
{
    public function __construct(readonly $a)
    {
    }
}
-----
<?php

namespace App;

trait Shared
{
    public readonly int $s;
}

trait Left
{
    use Shared;
}

trait Right
{
    use Shared;
}

trait Diamond
{
    use Left, Right;
}

final readonly class UsesDiamond
{
    use Diamond;

    public function __construct(public int $a, public int &$b)
    {
    }
}

abstract class Base
{
}

readonly class ReadonlyBase extends Base
{
}

final class NotAllParents extends ReadonlyBase
{
    public readonly int $a;
}

readonly class Open extends ReadonlyBase
{
    public function __construct(public $a)
    {
    }
}
