<?php

namespace Pun\Oracle\ReadonlyProperty;

final class Modifiers
{
    public function __construct(
        protected int $a,
        private ?int $b = null,
    ) {
    }
}

final class OrderAndDoc
{
    /** @readonly */
    private int $a;

    /**
     * @readonly
     * @var int
     */
    private int $b;

    public function __construct(int $a, int $b)
    {
        $this->a = $a;
        $this->b = $b;
    }
}

final class Writes
{
    private int $inc;
    private int $dec;
    private array $dim;
    private int $op;
    private int $static;
    private int $ok;

    public function __construct()
    {
        $this->inc = 1;
        $this->dec = 1;
        $this->dim = [];
        $this->op = 1;
        $this->static = 1;
        $this->ok = 1;
    }

    public function run(): int
    {
        $this->inc++;
        --$this->dec;
        $this->dim[] = 1;
        $this->op += 1;
        self::$static = 2;
        return $this->ok;
    }
}

final class Unset_
{
    private int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function run(): void
    {
        unset($this->a);
    }
}

final class Anonymous
{
    private int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function run(): object
    {
        return new class {
            private int $a;

            public function set(): void
            {
                $this->a = 2;
            }
        };
    }
}

final class ByRefFunction
{
    private array $a;
    private array $b;

    public function __construct()
    {
        $this->a = [3, 1];
        $this->b = [3, 1];
    }

    public function run(): void
    {
        sort($this->a);
        count($this->b);
    }
}

final class ClosureWrite
{
    public function __construct(private int $a)
    {
        $f = function () {
            $a = 2;
        };
    }
}

final class ParamReassign
{
    public function __construct(private int $a, private int $b)
    {
        $a = 2;
    }
}

final class WriteOther
{
    private int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function copy(self $other): void
    {
        $other->a = 2;
    }
}

final class ListWrite
{
    public function __construct(private int $a, private int $b)
    {
    }

    public function run(): void
    {
        [$this->a] = [1];
        list('x' => $this->b) = ['x' => 1];
    }
}
-----
<?php

namespace Pun\Oracle\ReadonlyProperty;

final class Modifiers
{
    public function __construct(
        protected int $a,
        private readonly ?int $b = null,
    ) {
    }
}

final class OrderAndDoc
{
    private readonly int $a;

    /**
     * @var int
     */
    private readonly int $b;

    public function __construct(int $a, int $b)
    {
        $this->a = $a;
        $this->b = $b;
    }
}

final class Writes
{
    private int $inc;
    private int $dec;
    private array $dim;
    private int $op;
    private int $static;
    private readonly int $ok;

    public function __construct()
    {
        $this->inc = 1;
        $this->dec = 1;
        $this->dim = [];
        $this->op = 1;
        $this->static = 1;
        $this->ok = 1;
    }

    public function run(): int
    {
        $this->inc++;
        --$this->dec;
        $this->dim[] = 1;
        $this->op += 1;
        self::$static = 2;
        return $this->ok;
    }
}

final class Unset_
{
    private int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function run(): void
    {
        unset($this->a);
    }
}

final class Anonymous
{
    private readonly int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function run(): object
    {
        return new class {
            private int $a;

            public function set(): void
            {
                $this->a = 2;
            }
        };
    }
}

final class ByRefFunction
{
    private array $a;
    private readonly array $b;

    public function __construct()
    {
        $this->a = [3, 1];
        $this->b = [3, 1];
    }

    public function run(): void
    {
        sort($this->a);
        count($this->b);
    }
}

final class ClosureWrite
{
    public function __construct(private readonly int $a)
    {
        $f = function () {
            $a = 2;
        };
    }
}

final class ParamReassign
{
    public function __construct(private int $a, private readonly int $b)
    {
        $a = 2;
    }
}

final class WriteOther
{
    private int $a;

    public function __construct()
    {
        $this->a = 1;
    }

    public function copy(self $other): void
    {
        $other->a = 2;
    }
}

final class ListWrite
{
    public function __construct(private int $a, private int $b)
    {
    }

    public function run(): void
    {
        [$this->a] = [1];
        list('x' => $this->b) = ['x' => 1];
    }
}
