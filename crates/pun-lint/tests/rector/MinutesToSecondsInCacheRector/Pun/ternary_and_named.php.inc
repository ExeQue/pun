<?php

use Illuminate\Support\Facades\Cache;

class More
{
    public function run(int $minutes)
    {
        Cache::put('a', 'b', $minutes ?: 1);
        Cache::put('a', 'b', seconds: 5);
    }
}
-----
<?php

use Illuminate\Support\Facades\Cache;

class More
{
    public function run(int $minutes)
    {
        Cache::put('a', 'b', ($minutes ?: 1) * 60);
        Cache::put('a', 'b', seconds: 5 * 60);
    }
}
