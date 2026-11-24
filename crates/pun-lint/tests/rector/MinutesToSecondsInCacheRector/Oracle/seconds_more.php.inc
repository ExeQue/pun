<?php

namespace App;

use Illuminate\Contracts\Cache\Repository;
use Illuminate\Contracts\Cache\Store;
use Illuminate\Support\Facades\Cache;

class More
{
    public function run(Repository $repository, Store $store, int $minutes, $untyped, float $f, int|float $n)
    {
        Cache::put('a', 'b', 5);
        Cache::put('a', 'b', $minutes + 1);
        Cache::add('a', 'b', $minutes);
        Cache::put('a', 'b', '5');
        Cache::put('a', 'b');
        $repository->add('a', 'b', $f);
        $repository->put('a', 'b', $untyped);
        $repository->remember('a', $n, fn () => 1);
        $repository->putMany(['a' => 1], 5);
        $store->putMany(['a' => 1], 5);
        $store->remember('a', 5, fn () => 1);
        Cache::PUT('a', 'b', 5);
        Cache::put('a', 'b', -5);
        Cache::put('a', 'b', $minutes * 60);
        Cache::put('a', 'b', ($minutes * 60));
    }
}
-----
<?php

namespace App;

use Illuminate\Contracts\Cache\Repository;
use Illuminate\Contracts\Cache\Store;
use Illuminate\Support\Facades\Cache;

class More
{
    public function run(Repository $repository, Store $store, int $minutes, $untyped, float $f, int|float $n)
    {
        Cache::put('a', 'b', 5 * 60);
        Cache::put('a', 'b', ($minutes + 1) * 60);
        Cache::add('a', 'b', $minutes * 60);
        Cache::put('a', 'b', '5');
        Cache::put('a', 'b');
        $repository->add('a', 'b', $f * 60);
        $repository->put('a', 'b', $untyped);
        $repository->remember('a', $n, fn () => 1);
        $repository->putMany(['a' => 1], 5);
        $store->putMany(['a' => 1], 5 * 60);
        $store->remember('a', 5, fn () => 1);
        Cache::PUT('a', 'b', 5 * 60);
        Cache::put('a', 'b', -5 * 60);
        Cache::put('a', 'b', $minutes * 60);
        Cache::put('a', 'b', ($minutes * 60));
    }
}
