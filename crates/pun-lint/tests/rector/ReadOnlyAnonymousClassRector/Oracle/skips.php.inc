<?php

namespace App\Skips;

class Writable
{
    public int $x = 1;
}

readonly class Frozen
{
}

readonly class ChildOfFrozen extends Frozen
{
}

$writableParent = new class extends Writable {
    private readonly int $p;
};

$unknownParent = new class extends Unknown {
    private readonly int $p;
};

$frozenGrandparent = new class extends ChildOfFrozen {
    private readonly int $p;
};

$empty = new class {
};

$emptyConstructor = new class {
    public function __construct()
    {
    }
};

$untypedParam = new class(1) {
    private readonly int $p;

    public function __construct($untyped)
    {
        $this->p = $untyped;
    }
};

$writablePromoted = new class(1, 2) {
    public function __construct(
        public int $writable,
        private readonly int $p,
    ) {
    }
};

$untypedProperty = new class {
    private $untyped;
};

$untypedReadonlyPromoted = new class(1) {
    public function __construct(private readonly $p)
    {
    }
};

$static = new class {
    public static int $count = 0;
};

$noProperties = new class extends Frozen implements \Countable {
    public function count(): int
    {
        return 0;
    }
};

$already = new readonly class {
    private readonly int $p;
};

$attributed = new #[\AllowDynamicProperties] class {
    private readonly int $p;
};
-----
<?php

namespace App\Skips;

class Writable
{
    public int $x = 1;
}

readonly class Frozen
{
}

readonly class ChildOfFrozen extends Frozen
{
}

$writableParent = new class extends Writable {
    private readonly int $p;
};

$unknownParent = new class extends Unknown {
    private readonly int $p;
};

$frozenGrandparent = new readonly class extends ChildOfFrozen {
    private int $p;
};

$empty = new class {
};

$emptyConstructor = new class {
    public function __construct()
    {
    }
};

$untypedParam = new class(1) {
    private readonly int $p;

    public function __construct($untyped)
    {
        $this->p = $untyped;
    }
};

$writablePromoted = new class(1, 2) {
    public function __construct(
        public int $writable,
        private readonly int $p,
    ) {
    }
};

$untypedProperty = new class {
    private $untyped;
};

$untypedReadonlyPromoted = new class(1) {
    public function __construct(private readonly $p)
    {
    }
};

$static = new class {
    public static int $count = 0;
};

$noProperties = new class extends Frozen implements \Countable {
    public function count(): int
    {
        return 0;
    }
};

$already = new readonly class {
    private readonly int $p;
};

$attributed = new #[\AllowDynamicProperties] class {
    private readonly int $p;
};
