<?php

namespace TraitInheritedPhpDoc;

use function PHPStan\dumpType;

interface Collects
{
    /**
     * @param array<int, string>|string $x
     * @return ($x is array ? array<int, static> : int)
     */
    public static function collect(array|string $x): array|int;
}

interface Counts
{
    /**
     * @return positive-int
     */
    public function size(): int;
}

abstract class Base
{
    /**
     * @return non-empty-string
     */
    public function label(): string
    {
        return 'x';
    }
}

trait Shared
{
    public static function collect(array|string $x): array|int
    {
        return 1;
    }

    public function size(): int
    {
        return 1;
    }

    public function label(): string
    {
        return 'y';
    }

    public function plain(): int
    {
        return 1;
    }
}

abstract class Parent_ implements Collects
{
    use Shared;
}

final class Child extends Parent_
{
}

final class Direct implements Collects
{
    use Shared;
}

final class Sized implements Counts
{
    use Shared;
}

final class Labelled extends Base
{
    use Shared;
}

final class Bare
{
    use Shared;
}

/** @param array<int, string> $a */
function f(array $a, Sized $sized, Labelled $labelled, Bare $bare): void
{
    dumpType(Child::collect($a));
    dumpType(Child::collect('s'));
    dumpType(Direct::collect($a));
    dumpType(Sized::collect($a));
    dumpType($sized->size());
    dumpType($labelled->label());
    dumpType($labelled->size());
    dumpType($bare->size());
    dumpType($bare->label());
    dumpType(Bare::collect($a));
    dumpType($bare->plain());
}
