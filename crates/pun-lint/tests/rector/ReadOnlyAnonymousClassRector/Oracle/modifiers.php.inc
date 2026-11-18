<?php

namespace App\Modifiers;

$promoted = new class(1, 2) {
    public function __construct(
        readonly int $a,
        private readonly int $b,
    ) {
    }
};

$properties = new class {
    readonly int $x;
    readonly public int $y;
    protected readonly string $z;

    public function __construct()
    {
        $this->x = 1;
        $this->y = 2;
        $this->z = 'z';
    }
};

$plain = new class(1) {
    public function __construct(private readonly int $p, int $plain = 0)
    {
    }
};

$many = new class {
    private readonly int $first, $second;
    public private(set) readonly string $set;
    private(set) readonly string $only;
    final public readonly string $final;
};

$setOnly = new class('a') {
    public function __construct(private(set) readonly string $s)
    {
    }
};

function outer(): object
{
    return new class {
        public function make(): object
        {
            return new class {
                private readonly int $inner;
            };
        }

        private readonly int $outer;
    };
}

$upper = new class(1) {
    public function __CONSTRUCT(readonly int $p)
    {
    }
};

$bare = new class {
    private readonly int $p;
};
-----
<?php

namespace App\Modifiers;

$promoted = new readonly class(1, 2) {
    public function __construct(
        public int $a,
        private int $b,
    ) {
    }
};

$properties = new readonly class {
    public int $x;
    public int $y;
    protected string $z;

    public function __construct()
    {
        $this->x = 1;
        $this->y = 2;
        $this->z = 'z';
    }
};

$plain = new readonly class(1) {
    public function __construct(private int $p, int $plain = 0)
    {
    }
};

$many = new readonly class {
    private int $first, $second;
    public private(set) string $set;
    public private(set) string $only;
    final public string $final;
};

$setOnly = new readonly class('a') {
    public function __construct(private(set) string $s)
    {
    }
};

function outer(): object
{
    return new readonly class {
        public function make(): object
        {
            return new readonly class {
                private int $inner;
            };
        }

        private int $outer;
    };
}

$upper = new readonly class(1) {
    public function __CONSTRUCT(public int $p)
    {
    }
};

$bare = new readonly class {
    private int $p;
};
