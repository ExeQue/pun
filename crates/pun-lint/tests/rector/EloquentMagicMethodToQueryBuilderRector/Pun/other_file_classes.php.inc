<?php

namespace App;

use RectorLaravel\Tests\Rector\StaticCall\EloquentMagicMethodToQueryBuilderRector\Source\DefaultMortgageSearchCache;
use RectorLaravel\Tests\Rector\StaticCall\EloquentMagicMethodToQueryBuilderRector\Source\User;

function run(): void
{
    $cached = DefaultMortgageSearchCache::latest();
    $other = DefaultMortgageSearchCache::where('a', 1);
    $active = User::where('active', true)->get();
    $sorted = User::orderBy('name', 'asc')->get();
}
-----
<?php

namespace App;

use RectorLaravel\Tests\Rector\StaticCall\EloquentMagicMethodToQueryBuilderRector\Source\DefaultMortgageSearchCache;
use RectorLaravel\Tests\Rector\StaticCall\EloquentMagicMethodToQueryBuilderRector\Source\User;

function run(): void
{
    $cached = DefaultMortgageSearchCache::latest();
    $other = DefaultMortgageSearchCache::where('a', 1);
    $active = User::query()->where('active', true)->get();
    $sorted = User::query()->orderBy('name', 'asc')->get();
}
