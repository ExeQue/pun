<?php

namespace PunModelSelfQuery;

use Illuminate\Database\Eloquent\Model;

use function PHPStan\Testing\assertType;

abstract class Base extends Model
{
    public static function statically(): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Base>', self::query());
        assertType('Illuminate\Database\Eloquent\Builder<static(PunModelSelfQuery\Base)>', static::query());
        assertType('PunModelSelfQuery\Base|null', self::query()->first());
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Base>', self::where('a', 1));
    }

    public function instance(self $model, Post $post): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Base>', $model::query());
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Base>', $this::query());
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Post>', $post::query());
    }
}

trait Finds
{
    public static function inTrait(): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Post>', self::query());
        assertType('Illuminate\Database\Eloquent\Builder<static(PunModelSelfQuery\Post)>', static::query());
    }

    public function inTraitInstance(): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Post>', $this::query());
    }
}

class Post extends Base
{
    use Finds;

    public static function fromParent(): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunModelSelfQuery\Base>', parent::query());
    }
}
