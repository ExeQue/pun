<?php

namespace TraitPrivateMembers;

trait LazyProxy
{
    private ?\Closure $initializer = null;

    public static function create(\Closure $initializer, ?self $instance = null): static
    {
        $instance ??= (new \ReflectionClass(static::class))->newInstanceWithoutConstructor();
        $instance->initializer = $initializer;

        return $instance;
    }

    public function steal(self $other): ?\Closure
    {
        return $other->initializer;
    }
}

class First
{
    use LazyProxy;
}

class Second
{
    use LazyProxy;

    public function fromFirst(First $first): ?\Closure
    {
        return $first->initializer;
    }
}

class SecondChild extends Second
{
    public function fromParent(Second $second): ?\Closure
    {
        return $second->initializer;
    }
}
