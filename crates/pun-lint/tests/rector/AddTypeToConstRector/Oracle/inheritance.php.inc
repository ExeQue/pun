<?php

namespace App\Inheritance;

interface HasName
{
    public const NAME = 'n';
}

class Base implements HasName
{
    private const PRIV = 'x';
    public const BASE = 1;
}

final class Child extends Base
{
    public const NAME = 'other', OWN = 'own';
    public const BASE = 2;
    private const PRIV = 1;
    public const base = 1;
}

class Open
{
    public const PUB = 'p';
    protected const PROT = 'p';
    private const PRIV = self::PUB;
    final protected const FIN = self::PROT;
}

abstract class Abs
{
    final public const FIN = 'f';
    private const PRIV = 1;
}

$anon = new class {
    public const PUB = 1;
    private const X = 1;
};

$child = new class extends Base {
    private const PRIV = 1;
    private const OWN = 1;
};

trait T
{
    private const TT = 1;
}

trait U
{
    use T;
}

final class UsesTraitOfTrait
{
    use U;

    public const TT = 1;
    public const NOT = 'x';
}

interface I
{
    public const IN_INTERFACE = 1;
}

trait InTrait
{
    public const IN_TRAIT = 1;
}

enum E
{
    public const IN_ENUM = 1;
}
-----
<?php

namespace App\Inheritance;

interface HasName
{
    public const NAME = 'n';
}

class Base implements HasName
{
    private const string PRIV = 'x';
    public const BASE = 1;
}

final class Child extends Base
{
    public const string NAME = 'other', OWN = 'own';
    public const BASE = 2;
    private const PRIV = 1;
    public const int base = 1;
}

class Open
{
    public const PUB = 'p';
    protected const PROT = 'p';
    private const string PRIV = self::PUB;
    final protected const string FIN = self::PROT;
}

abstract class Abs
{
    final public const FIN = 'f';
    private const int PRIV = 1;
}

$anon = new class {
    public const PUB = 1;
    private const int X = 1;
};

$child = new class extends Base {
    private const PRIV = 1;
    private const int OWN = 1;
};

trait T
{
    private const TT = 1;
}

trait U
{
    use T;
}

final class UsesTraitOfTrait
{
    use U;

    public const TT = 1;
    public const string NOT = 'x';
}

interface I
{
    public const IN_INTERFACE = 1;
}

trait InTrait
{
    public const IN_TRAIT = 1;
}

enum E
{
    public const IN_ENUM = 1;
}
