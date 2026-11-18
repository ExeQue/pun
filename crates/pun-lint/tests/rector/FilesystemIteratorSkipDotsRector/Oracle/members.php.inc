<?php

namespace App;

enum Mode: int
{
    case A = 1;
    public const SKIP = 4096;
}

class Base extends \FilesystemIterator
{
    public const MINE = 4096;
}

final class Child extends Base
{
    public const OWN = 2;

    public function run($class, $name)
    {
        new self(__DIR__, self::OWN);
        new static(__DIR__, static::SKIP_DOTS);
        new static(__DIR__, static::KEY_AS_FILENAME);
        new parent(__DIR__, parent::MINE);
        new parent(__DIR__, parent::KEY_AS_FILENAME);
        new $class(__DIR__, \FilesystemIterator::KEY_AS_FILENAME);
        new \FilesystemIterator(__DIR__, Mode::SKIP);
        new \FilesystemIterator(__DIR__, \FilesystemIterator::{$name});
        new \FilesystemIterator(__DIR__, $class::SKIP_DOTS);
    }
}

new \FilesystemIterator(__DIR__, self::KEY_AS_FILENAME);
-----
<?php

namespace App;

enum Mode: int
{
    case A = 1;
    public const SKIP = 4096;
}

class Base extends \FilesystemIterator
{
    public const MINE = 4096;
}

final class Child extends Base
{
    public const OWN = 2;

    public function run($class, $name)
    {
        new self(__DIR__, self::OWN | \FilesystemIterator::SKIP_DOTS);
        new static(__DIR__, static::SKIP_DOTS);
        new static(__DIR__, static::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
        new parent(__DIR__, parent::MINE);
        new parent(__DIR__, parent::KEY_AS_FILENAME);
        new $class(__DIR__, \FilesystemIterator::KEY_AS_FILENAME);
        new \FilesystemIterator(__DIR__, Mode::SKIP | \FilesystemIterator::SKIP_DOTS);
        new \FilesystemIterator(__DIR__, \FilesystemIterator::{$name} | \FilesystemIterator::SKIP_DOTS);
        new \FilesystemIterator(__DIR__, $class::SKIP_DOTS | \FilesystemIterator::SKIP_DOTS);
    }
}

new \FilesystemIterator(__DIR__, self::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
