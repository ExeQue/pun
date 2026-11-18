<?php

namespace App;

class Base extends \FilesystemIterator
{
    public const MINE = 4096;
    public const NAME = 'x';
}

final class Child extends Base
{
    public function run()
    {
        new self(__DIR__, parent::MINE);
        new self(__DIR__, parent::KEY_AS_FILENAME);
        new self(__DIR__, self::NAME);
    }
}
-----
<?php

namespace App;

class Base extends \FilesystemIterator
{
    public const MINE = 4096;
    public const NAME = 'x';
}

final class Child extends Base
{
    public function run()
    {
        new self(__DIR__, parent::MINE);
        new self(__DIR__, parent::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
        new self(__DIR__, self::NAME | \FilesystemIterator::SKIP_DOTS);
    }
}
