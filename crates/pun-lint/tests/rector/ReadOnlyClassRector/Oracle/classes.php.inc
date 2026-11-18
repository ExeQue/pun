<?php

namespace App;

readonly class Base
{
}

#[\AllowDynamicProperties]
readonly class Dyn
{
}

trait ReadonlyTrait
{
    public readonly string $t;
}

trait WritableTrait
{
    public string $w;
}

trait EmptyTrait
{
}

trait NestedTrait
{
    use WritableTrait;
}

final class A extends Base
{
    public function __construct(private readonly int $a)
    {
    }
}

abstract class B extends Base
{
    public string $writable;
    public static $s;
    var $v;
}

class C extends Base
{
}

final class D
{
    use ReadonlyTrait;

    public function __construct(protected readonly int $d)
    {
    }
}

final class E
{
    use WritableTrait;

    public function __construct(protected readonly int $e)
    {
    }
}

final class F
{
    use EmptyTrait;

    public readonly int $f;
}

final class G
{
    use NestedTrait;

    public readonly int $g;
}

final class H
{
    public function __construct(int $x)
    {
    }
}

final class I
{
    public function __construct($x, public readonly int $y)
    {
    }
}

final class J
{
    public function __CONSTRUCT(public readonly int $y)
    {
    }
}

final class K
{
    public  readonly   int $a, $b;
    protected readonly ?int $c;
}

final   class L
{
    public readonly int $a;
}

final class M extends \Exception
{
    public readonly int $a;
}

final class N
{
    public function __construct(
        public readonly int $a,
        readonly public int $b,
        public private(set) readonly int $c,
    ) {
    }
}

final class O
{
    public function __construct(
        public readonly int $a = 1,
    ) {
    }
}

final class P
{
    public readonly int $a;

    public function __construct(...$rest)
    {
    }
}

final class Q extends Dyn
{
    public readonly int $a;
}

/** doc */
final class R
{
    public readonly int $a;
}

final class S
{
    use UnknownTrait;

    public readonly int $a;
}
-----
<?php

namespace App;

readonly class Base
{
}

#[\AllowDynamicProperties]
readonly class Dyn
{
}

trait ReadonlyTrait
{
    public readonly string $t;
}

trait WritableTrait
{
    public string $w;
}

trait EmptyTrait
{
}

trait NestedTrait
{
    use WritableTrait;
}

final readonly class A extends Base
{
    public function __construct(private int $a)
    {
    }
}

abstract readonly class B extends Base
{
    public string $writable;
    public static $s;
    public $v;
}

readonly class C extends Base
{
}

final readonly class D
{
    use ReadonlyTrait;

    public function __construct(protected int $d)
    {
    }
}

final class E
{
    use WritableTrait;

    public function __construct(protected readonly int $e)
    {
    }
}

final readonly class F
{
    use EmptyTrait;

    public int $f;
}

final class G
{
    use NestedTrait;

    public readonly int $g;
}

final readonly class H
{
    public function __construct(int $x)
    {
    }
}

final class I
{
    public function __construct($x, public readonly int $y)
    {
    }
}

final readonly class J
{
    public function __CONSTRUCT(public int $y)
    {
    }
}

final readonly class K
{
    public int $a, $b;
    protected ?int $c;
}

final readonly class L
{
    public int $a;
}

final class M extends \Exception
{
    public readonly int $a;
}

final readonly class N
{
    public function __construct(
        public int $a,
        public int $b,
        public private(set) int $c,
    ) {
    }
}

final readonly class O
{
    public function __construct(
        public int $a = 1,
    ) {
    }
}

final class P
{
    public readonly int $a;

    public function __construct(...$rest)
    {
    }
}

final readonly class Q extends Dyn
{
    public int $a;
}

/** doc */
final readonly class R
{
    public int $a;
}

final class S
{
    use UnknownTrait;

    public readonly int $a;
}
