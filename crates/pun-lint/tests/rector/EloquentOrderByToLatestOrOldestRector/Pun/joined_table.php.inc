<?php

// Rector also turns `orderByDesc('f.filed_on')` on an Eloquent builder into `latest('f.filed_on')`. Larastan types
// `latest()`'s column as a property of the builder's model and, with `checkModelProperties`, rejects a column of a
// joined table ("Database table "f" does not have column "filed_on""), so Pun leaves a qualified column on an
// Eloquent builder alone. A plain query builder and an unqualified column are still rewritten.

namespace App;

use Illuminate\Database\Query\Builder;
use UserLand\SomeModel;

function run(Builder $query)
{
    // After `->join('filings as f', ...)`.
    SomeModel::query()->orderByDesc('f.filed_on');
    SomeModel::query()->orderByDesc('filed_on');
    $query->orderByDesc('f.filed_on');
}
-----
<?php

// Rector also turns `orderByDesc('f.filed_on')` on an Eloquent builder into `latest('f.filed_on')`. Larastan types
// `latest()`'s column as a property of the builder's model and, with `checkModelProperties`, rejects a column of a
// joined table ("Database table "f" does not have column "filed_on""), so Pun leaves a qualified column on an
// Eloquent builder alone. A plain query builder and an unqualified column are still rewritten.

namespace App;

use Illuminate\Database\Query\Builder;
use UserLand\SomeModel;

function run(Builder $query)
{
    // After `->join('filings as f', ...)`.
    SomeModel::query()->orderByDesc('f.filed_on');
    SomeModel::query()->latest('filed_on');
    $query->latest('f.filed_on');
}
