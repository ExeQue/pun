<?php

namespace PunBuilderMacros;

use Illuminate\Database\Eloquent\Builder as EloquentBuilder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Query\Builder;
use Illuminate\Support\Collection;

use function PHPStan\Testing\assertType;

class Item extends Model
{
}

class Macros
{
    public static function register(): void
    {
        Builder::macro('punInt', function (): int {
            return 1;
        });
        Builder::macro('punQuery', function (): Builder {
            return $this->limit(1);
        });
        Builder::macro('punNullable', fn (): ?string => null);
        Builder::macro('punCollection', fn (): Collection => new Collection());
        Builder::macro('punUnknown', function () {
            return 1;
        });
        Builder::macro('punMaybe', fn (): ?static => null);
        Builder::macro('punCallable', [self::class, 'listed']);
        Builder::macro('punString', 'PunBuilderMacros\Macros::listed');
        EloquentBuilder::macro('punOwn', fn (): bool => true);
    }

    /** @return list<string> */
    public static function listed(): array
    {
        return [];
    }
}

function test(Builder $query): void
{
    assertType('int', Item::query()->punInt());
    assertType('int', Item::where('a', 1)->punInt());
    assertType('int', Item::query()->toBase()->punInt());
    assertType('Illuminate\Database\Query\Builder', Item::query()->punQuery());
    assertType('string|null', Item::query()->punNullable());
    assertType('Illuminate\Support\Collection', Item::query()->punCollection());
    assertType('mixed', Item::query()->punUnknown());
    assertType('Illuminate\Database\Eloquent\Builder<PunBuilderMacros\Item>|null', Item::query()->punMaybe());
    assertType('Illuminate\Database\Eloquent\Builder<PunBuilderMacros\Item>', Item::query()->punCallable());
    assertType('Illuminate\Database\Eloquent\Builder<PunBuilderMacros\Item>', Item::query()->punString());
    assertType('bool', Item::query()->punOwn());
    assertType('int', $query->punInt());
    assertType('Illuminate\Database\Query\Builder|null', $query->punMaybe());
    assertType('list<string>', $query->punCallable());
    assertType('list<string>', $query->punString());
}
