<?php

namespace App;

use Illuminate\Contracts\Database\Query\Builder;

class C
{
    public function run(Builder $query, $other, string $name)
    {
        $query->where($name);
        $query->where(123);
        $query->where('a', '<>', null);
        $query->where('a', '!=', null, 'or');
        $query->WHERE('b', NULL);
        $other->where('c', null);
        $query->where(column: 'd', value: null);
        $x = $query->where('e', null)->get();
        $query
            ->where('f', '!=', null)
            ->get();
    }
}
-----
<?php

namespace App;

use Illuminate\Contracts\Database\Query\Builder;

class C
{
    public function run(Builder $query, $other, string $name)
    {
        $query->whereNull($name);
        $query->where(123);
        $query->whereNull('a');
        $query->where('a', '!=', null, 'or');
        $query->whereNull('b');
        $other->where('c', null);
        $query->whereNull(column: 'd');
        $x = $query->whereNull('e')->get();
        $query->whereNotNull('f')
            ->get();
    }
}
