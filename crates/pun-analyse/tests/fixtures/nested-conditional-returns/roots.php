<?php

// symfony/config's TreeBuilder::getRootNode(): an eight-level conditional return type on a class template.

namespace NestedConditionalReturns;

/**
 * @template TParent
 */
class Node
{
    /** @param TParent $parent */
    public function __construct(private $parent)
    {
    }

    /** @return TParent */
    public function end()
    {
        return $this->parent;
    }
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class ArrNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class VariableNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class ScalarNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class StrNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class BooleanNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class IntegerNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class FltNode extends Node
{
}

/**
 * @template TParent
 *
 * @extends Node<TParent>
 */
class EnumNode extends Node
{
}

/**
 * @template T of 'array'|'variable'|'scalar'|'string'|'boolean'|'integer'|'float'|'enum' = 'array'
 */
class Builder
{
    /** @param T $type */
    public function __construct(string $type = 'array')
    {
    }

    /** @return (T is 'array' ? ArrNode<$this> : (T is 'variable' ? VariableNode<$this> : (T is 'scalar' ? ScalarNode<$this> : (T is 'string' ? StrNode<$this> : (T is 'boolean' ? BooleanNode<$this> : (T is 'integer' ? IntegerNode<$this> : (T is 'float' ? FltNode<$this> : (T is 'enum' ? EnumNode<$this> : Node<$this>)))))))) */
    public function root(): Node
    {
        return new Node($this);
    }

    public function inside(): void
    {
        \PHPStan\dumpType($this->root());
    }
}

