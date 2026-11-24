<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class FixtureHelper extends TestCase
{
    private $a;

    public function helper()
    {
    }
}

final class ConstructedHelper extends TestCase
{
    public function __CONSTRUCT()
    {
        parent::__construct(static::class);
    }
}

class SomeTests extends TestCase
{
}

final class NotAHelper
{
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class FixtureHelper extends TestCase
{
    public function __construct()
    {
        parent::__construct(static::class);
    }
    private $a;

    public function helper()
    {
    }
}

final class ConstructedHelper extends TestCase
{
    public function __CONSTRUCT()
    {
        parent::__construct(static::class);
    }
}

class SomeTests extends TestCase
{
    public function __construct()
    {
        parent::__construct(static::class);
    }
}

final class NotAHelper
{
}
