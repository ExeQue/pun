<?php

namespace PunGenericEloquentUnions;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Collection;
use Illuminate\Database\Eloquent\Model;

use function PHPStan\Testing\assertType;

/**
 * @method static \Illuminate\Database\Eloquent\Builder|Item query()
 * @method static Builder|Item popular()
 * @method static Builder<static>|Item newModelQuery()
 * @method static Builder|Item|null unpopular()
 * @property-read ItemBuilder|Item $plain
 * @property-read Collection|Item[] $children
 * @property-read Collection|array<Item> $generic
 */
class Item extends Model
{
    /** @param Builder|Item $query */
    public function scopeNamed($query): void
    {
        assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>', $query);
    }
}

/** @extends Builder<Item> */
class ItemBuilder extends Builder
{
}

/**
 * @template TModel of Model
 * @param class-string<TModel> $class
 */
function generic(string $class): void
{
    assertType('Illuminate\Database\Eloquent\Builder<TModel of Illuminate\Database\Eloquent\Model (function PunGenericEloquentUnions\generic(), argument)>', $class::query());
    assertType('Illuminate\Database\Eloquent\Builder<TModel of Illuminate\Database\Eloquent\Model (function PunGenericEloquentUnions\generic(), argument)>', $class::query()->where('a', 1));
}

/** @return Builder|Item */
function builder()
{
    return Item::query();
}

function test(Item $item): void
{
    assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>', Item::query());
    assertType('PunGenericEloquentUnions\Item', Item::query()->findOrFail(1));
    assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>', Item::popular());
    assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>|PunGenericEloquentUnions\Item', Item::newModelQuery());
    assertType('Illuminate\Database\Eloquent\Builder|PunGenericEloquentUnions\Item|null', Item::unpopular());
    assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>', builder());
    assertType('PunGenericEloquentUnions\ItemBuilder', $item->plain);
    assertType('Illuminate\Database\Eloquent\Collection<int, PunGenericEloquentUnions\Item>', $item->children);
    assertType('PunGenericEloquentUnions\Item|null', $item->children->first());
    assertType('array<PunGenericEloquentUnions\Item>|Illuminate\Database\Eloquent\Collection', $item->generic);
}

class Holder
{
    /** @var Builder|Item */
    public $query;
}

/**
 * @template TModel of Model
 * @param class-string<TModel> $class
 * @return builder-of<TModel>|null
 */
function maybeQuery(string $class, bool $none)
{
    return $none ? null : $class::query();
}

/**
 * @template TModel of Model
 * @param class-string<TModel> $class
 */
function maybeGeneric(string $class, Holder $holder): void
{
    assertType('Illuminate\Database\Eloquent\Builder<PunGenericEloquentUnions\Item>', $holder->query);
    assertType('Illuminate\Database\Eloquent\Builder<TModel of Illuminate\Database\Eloquent\Model (function PunGenericEloquentUnions\maybeGeneric(), argument)>|null', maybeQuery($class, false));
}
