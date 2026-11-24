<?php

namespace App;

class More
{
    public function run($fmt, $x)
    {
        $b = date($fmt);
        $c = date('Y', $x);
        $d = strtotime($x);
        $e = strtotime('x', 'y');
        $f = time() - 0;
        $g = time() - (2 * (3 * 5));
        $h = time() - $x;
        $i = time() - 90;
        $j = \DATE('Y');
        $k = date(format: 'Y');
        $m = time() - 2.5;
        $o = time() - 60 * 60 * 24 * 7 * 2;
        $p = (time()) - 7200;
        $q = date("Y-m-d", (strtotime('tomorrow')));
        $r = time() + 60;
        $s = date('Y', strtotime(...));
        $t = strtotime();
        $u = date();
        $w = date("Y $fmt");
        $y = $this->date('Y');
        $z = time() - 86400 * 3;
    }
}
-----
<?php

namespace App;

class More
{
    public function run($fmt, $x)
    {
        $b = date($fmt);
        $c = \Carbon\Carbon::createFromTimestamp($x)->format('Y');
        $d = \Carbon\Carbon::parse($x)->getTimestamp();
        $e = strtotime('x', 'y');
        $f = \Carbon\Carbon::now()->subWeeks(0)->getTimestamp();
        $g = \Carbon\Carbon::now()->subSeconds(30)->getTimestamp();
        $h = time() - $x;
        $i = \Carbon\Carbon::now()->subSeconds(90)->getTimestamp();
        $j = \Carbon\Carbon::now()->format('Y');
        $k = \Carbon\Carbon::now()->format('Y');
        $m = time() - 2.5;
        $o = \Carbon\Carbon::now()->subWeeks(2)->getTimestamp();
        $p = \Carbon\Carbon::now()->subHours(2)->getTimestamp();
        $q = \Carbon\Carbon::parse('tomorrow')->format("Y-m-d");
        $r = time() + 60;
        $s = \Carbon\Carbon::createFromTimestamp(strtotime(...))->format('Y');
        $t = strtotime();
        $u = date();
        $w = date("Y $fmt");
        $y = $this->date('Y');
        $z = \Carbon\Carbon::now()->subDays(3)->getTimestamp();
    }
}
