<?php

namespace App;

use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;

final class Team extends Model
{
    /**
     * @param Builder<static> $query
     * @return Builder<self>
     */
    public function scopeActive(Builder $query): Builder
    {
        return $query;
    }

    public function scopeUntyped($query)
    {
        return $query;
    }

    public function scopeOther(Builder $query): ?Builder
    {
        return $query;
    }

    public function scopeNone()
    {
    }

    public function scopex(Builder $query)
    {
    }

    public function scopeString(string $query)
    {
    }

    #[Scope]
    protected function popular($query): \Illuminate\Database\Eloquent\Builder
    {
        return $query;
    }
}

trait Scopes
{
    public function scopeTrait(Builder $query): Builder
    {
        return $query;
    }
}

$model = new class extends Model {
    public function scopeAnon(Builder $query): Builder
    {
        return $query;
    }
};

abstract class Base extends Model
{
    abstract public function scopeAbstract(Builder $query): Builder;
}
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;

final class Team extends Model
{
    /**
     * @param \Illuminate\Database\Eloquent\Builder<self> $query
     * @return Builder<self>
     */
    public function scopeActive(Builder $query): Builder
    {
        return $query;
    }

    /**
     * @param \Illuminate\Database\Eloquent\Builder<self> $query
     */
    public function scopeUntyped($query)
    {
        return $query;
    }

    /**
     * @param \Illuminate\Database\Eloquent\Builder<self> $query
     */
    public function scopeOther(Builder $query): ?Builder
    {
        return $query;
    }

    public function scopeNone()
    {
    }

    public function scopex(Builder $query)
    {
    }

    public function scopeString(string $query)
    {
    }

    /**
     * @param \Illuminate\Database\Eloquent\Builder<self> $query
     * @return \Illuminate\Database\Eloquent\Builder<self>
     */
    #[Scope]
    protected function popular($query): \Illuminate\Database\Eloquent\Builder
    {
        return $query;
    }
}

trait Scopes
{
    public function scopeTrait(Builder $query): Builder
    {
        return $query;
    }
}

$model = new class extends Model {
    public function scopeAnon(Builder $query): Builder
    {
        return $query;
    }
};

abstract class Base extends Model
{
    abstract public function scopeAbstract(Builder $query): Builder;
}
