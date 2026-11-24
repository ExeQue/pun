<?php

namespace App;

class More
{
    public function run()
    {
        $a = time();
        $b = \TIME();
        $c = array_map(time(...), []);
        $d = microtime();
        $e = $this->time();
    }
}
-----
<?php

namespace App;

class More
{
    public function run()
    {
        $a = \Carbon\Carbon::now()->getTimestamp();
        $b = \Carbon\Carbon::now()->getTimestamp();
        $c = array_map(static fn() => \Carbon\Carbon::now()->getTimestamp(), []);
        $d = microtime();
        $e = $this->time();
    }
}
