<?php

final class Shapes
{
    public ?Shapes $next = null;
    public string $name = '';
    public static ?Shapes $instance = null;

    public function name(): string
    {
        return $this->name;
    }

    public function maybe(): ?string
    {
        return null;
    }

    public function run(?Shapes $a, ?Shapes $b): void
    {
        $x = $a !== null ? $a->name() : 'none';
        $x = null === $a ? 'none' : $a->name();
        $x = $a !== null ? $a->next->name : 'none';
        $x = $a !== null ? $a->name()->foo : null;
        $x = $a !== null ? $a->maybe() : null;
        $x = $a !== null ? $a->maybe() : 'none';
        $x = $a !== null ? $b->name() : null;
        $x = self::$instance !== null ? self::$instance->name() : 'none';
        $x = $a->next !== null ? $a->next->name() : 'none';
        $x = 'p' . ($a !== null ? $a->name() : 'none');
        $x = $a !== null ? $a->name : 'none';
        $x = $a != null ? $a->name() : 'none';
        $x = $a !== null ?: 'none';
        $x = $a?->next !== null ? $a->next->name() : 'none';
    }
}
-----
<?php

final class Shapes
{
    public ?Shapes $next = null;
    public string $name = '';
    public static ?Shapes $instance = null;

    public function name(): string
    {
        return $this->name;
    }

    public function maybe(): ?string
    {
        return null;
    }

    public function run(?Shapes $a, ?Shapes $b): void
    {
        $x = $a?->name() ?? 'none';
        $x = $a?->name() ?? 'none';
        $x = $a->next->name ?? 'none';
        $x = $a?->name()->foo;
        $x = $a?->maybe();
        $x = $a !== null ? $a->maybe() : 'none';
        $x = $a !== null ? $b->name() : null;
        $x = self::$instance?->name() ?? 'none';
        $x = $a->next?->name() ?? 'none';
        $x = 'p' . ($a?->name() ?? 'none');
        $x = $a->name ?? 'none';
        $x = $a != null ? $a->name() : 'none';
        $x = $a !== null ?: 'none';
        $x = $a?->next !== null ? $a->next->name() : 'none';
    }
}
