<?php

namespace App;

use Illuminate\Contracts\Database\Query\Builder;

class More
{
    public function run(Builder $query, string $op)
    {
        $query->where('name', 'LIKE', 'a%');
        $query->where('name', 'like binary', value: 'a%');
        $query->where('name', 'not like binary', 'a%');
        $query->where('name', 'ilike', 'a%');
        $query->where('name', 'not ilike', 'a%');
        $query->where('name', $op, 'a%');
        $query->where('name', 'like', 'a%', 'or');
        $query->where('name', 'like');
        $query->where('name', "like", 'a');
        $query->where(
            'name',
            'like',
            'a%'
        );
    }
}
-----
<?php

namespace App;

use Illuminate\Contracts\Database\Query\Builder;

class More
{
    public function run(Builder $query, string $op)
    {
        $query->whereLike('name', 'a%');
        $query->whereLike('name', value: 'a%', caseSensitive: true);
        $query->whereNotLike('name', 'a%', true);
        $query->whereLike('name', 'a%');
        $query->whereNotLike('name', 'a%');
        $query->where('name', $op, 'a%');
        $query->where('name', 'like', 'a%', 'or');
        $query->where('name', 'like');
        $query->whereLike('name', 'a');
        $query->whereLike(
            'name',
            'a%'
        );
    }
}
