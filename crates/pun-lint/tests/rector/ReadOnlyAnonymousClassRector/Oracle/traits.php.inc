<?php

namespace App\Traits;

trait ReadonlyTrait
{
    public readonly int $t;
}

trait WritableTrait
{
    public int $w;
}

trait EmptyTrait
{
    public function hello(): void
    {
    }
}

trait NestedWritable
{
    use WritableTrait;
}

$readonlyTrait = new class {
    use ReadonlyTrait;

    private readonly int $p;
};

$writableTrait = new class {
    use WritableTrait;

    private readonly int $p;
};

$emptyTrait = new class {
    use EmptyTrait;

    private readonly int $p;
};

$nested = new class {
    use NestedWritable;

    private readonly int $p;
};

$unknownTrait = new class {
    use UnknownTrait;

    private readonly int $p;
};

$both = new class {
    use EmptyTrait, ReadonlyTrait;

    private readonly int $p;
};
-----
<?php

namespace App\Traits;

trait ReadonlyTrait
{
    public readonly int $t;
}

trait WritableTrait
{
    public int $w;
}

trait EmptyTrait
{
    public function hello(): void
    {
    }
}

trait NestedWritable
{
    use WritableTrait;
}

$readonlyTrait = new readonly class {
    use ReadonlyTrait;

    private int $p;
};

$writableTrait = new class {
    use WritableTrait;

    private readonly int $p;
};

$emptyTrait = new readonly class {
    use EmptyTrait;

    private int $p;
};

$nested = new class {
    use NestedWritable;

    private readonly int $p;
};

$unknownTrait = new class {
    use UnknownTrait;

    private readonly int $p;
};

$both = new readonly class {
    use EmptyTrait, ReadonlyTrait;

    private int $p;
};
