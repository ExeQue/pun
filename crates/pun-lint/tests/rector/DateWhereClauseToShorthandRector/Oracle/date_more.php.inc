<?php

namespace App;

use Carbon\Carbon;
use Carbon\CarbonImmutable;
use Illuminate\Database\Query\Builder;

final class More
{
    public function run(Builder $query): void
    {
        $query->where('a', '<', \now());
        $query->where('a', '<', now(), 'or');
        $query->where('a', "<", now());
        $query->WHERE('a', '<', now());
        $query->whereDate('a', '>', CarbonImmutable::today());
        $query->whereDate('a', '>', \DateTime::today());
        $query->where(
            'a',
            '>',
            now()
        );
        $query->whereDate('a', today())->get();
    }
}
-----
<?php

namespace App;

use Carbon\Carbon;
use Carbon\CarbonImmutable;
use Illuminate\Database\Query\Builder;

final class More
{
    public function run(Builder $query): void
    {
        $query->wherePast('a');
        $query->where('a', '<', now(), 'or');
        $query->wherePast('a');
        $query->WHERE('a', '<', now());
        $query->whereDate('a', '>', CarbonImmutable::today());
        $query->whereAfterToday('a');
        $query->whereFuture(
            'a'
        );
        $query->whereToday('a')->get();
    }
}
