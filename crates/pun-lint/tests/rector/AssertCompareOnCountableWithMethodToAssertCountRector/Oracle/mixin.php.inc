<?php

namespace Pun\MixinCount;

use PHPUnit\Framework\TestCase;

class QueryBuilder
{
    /** @return $this */
    public function whereExists()
    {
        return $this;
    }
}

/** @mixin QueryBuilder */
class EloquentBuilder
{
    public function __call($method, $parameters)
    {
    }

    public function get(): \ArrayObject
    {
        return new \ArrayObject();
    }
}

final class MixinCountTest extends TestCase
{
    public function test(EloquentBuilder $query): void
    {
        $this->assertEquals(2, $query->whereExists()->get()->count());
        $this->assertEquals(2, $query->get()->count());
    }
}
-----
<?php

namespace Pun\MixinCount;

use PHPUnit\Framework\TestCase;

class QueryBuilder
{
    /** @return $this */
    public function whereExists()
    {
        return $this;
    }
}

/** @mixin QueryBuilder */
class EloquentBuilder
{
    public function __call($method, $parameters)
    {
    }

    public function get(): \ArrayObject
    {
        return new \ArrayObject();
    }
}

final class MixinCountTest extends TestCase
{
    public function test(EloquentBuilder $query): void
    {
        $this->assertEquals(2, $query->whereExists()->get()->count());
        $this->assertCount(2, $query->get());
    }
}
