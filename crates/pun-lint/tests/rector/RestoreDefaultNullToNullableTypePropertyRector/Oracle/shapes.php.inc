<?php

class Shapes
{
    public mixed $mixed;
    public ?int $nullable;
    public int|null $union;
    public null $null;
    public (Countable&Traversable)|null $dnf;
    public ?int $a, $b;
    public int $plain;
    protected static ?string $static;
    var ?string $var;
    /** @readonly */
    public ?string $docReadonly;
    public ?string $conditional;
    public ?string $loop;
    public ?string $constructorOnly;

    public function __construct(bool $flag, array $items)
    {
        if ($flag) {
            $this->conditional = 'x';
        }
        $this->constructorOnly = 'y';
    }

    public function other()
    {
        $this->loop = 'z';
    }
}

/**
 * @immutable
 */
class Immutable
{
    public ?int $value;
}

final class Promoted
{
    public ?int $value;

    public function __construct(public ?int $promoted)
    {
    }
}
-----
<?php

class Shapes
{
    public mixed $mixed;
    public ?int $nullable = null;
    public int|null $union = null;
    public null $null = null;
    public (Countable&Traversable)|null $dnf = null;
    public ?int $a, $b;
    public int $plain;
    protected static ?string $static;
    var ?string $var = null;
    /** @readonly */
    public ?string $docReadonly;
    public ?string $conditional;
    public ?string $loop = null;
    public ?string $constructorOnly;

    public function __construct(bool $flag, array $items)
    {
        if ($flag) {
            $this->conditional = 'x';
        }
        $this->constructorOnly = 'y';
    }

    public function other()
    {
        $this->loop = 'z';
    }
}

/**
 * @immutable
 */
class Immutable
{
    public ?int $value;
}

final class Promoted
{
    public ?int $value = null;

    public function __construct(public ?int $promoted)
    {
    }
}
