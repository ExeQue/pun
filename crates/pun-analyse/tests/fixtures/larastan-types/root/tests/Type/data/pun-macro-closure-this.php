<?php

namespace PunMacroClosureThis;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Support\Str;
use Illuminate\Support\Traits\Macroable;

use function PHPStan\Testing\assertType;

class Registry
{
    use Macroable;

    public function size(): int
    {
        return 0;
    }

    /** @param-closure-this $this $callback */
    public function tap(\Closure $callback): void
    {
    }
}

class Provider
{
    public function boot(Registry $registry): void
    {
        Registry::macro('double', function (): int {
            assertType('PunMacroClosureThis\Registry', $this);

            return $this->size() * 2;
        });
        Str::macro('shout', function (string $value): string {
            assertType('Illuminate\Support\Str', $this);

            return $value;
        });
        Builder::macro('addSubSelect', function (string $column, Builder $query) {
            /** @var Builder<\Illuminate\Database\Eloquent\Model> $this */
            if (is_null($this->getQuery()->columns)) {
                $this->select('*');
            }

            return $this->selectSub($query->limit(1)->getQuery(), $column);
        });
        $registry->tap(function (): void {
            assertType('PunMacroClosureThis\Registry', $this);
            $this->size();
        });
    }
}
