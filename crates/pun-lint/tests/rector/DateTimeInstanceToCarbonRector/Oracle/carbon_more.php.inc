<?php

namespace App;

use DateTime;
use DateTimeImmutable as Imm;

class More
{
    public function run($s)
    {
        $a = new DateTime;
        $b = new Imm('+2 hours -3 min');
        $c = new \datetime("yesterday +1 week");
        $d = new DateTime($s);
        $e = new DateTime('now', null);
        $f = new \DateTimeZone('UTC');
        $g = new DateTime('next monday +5 days');
        $h = (new DateTime('tomorrow'))->modify('+1 day');
        $i = new DateTime('+007 secs');
        $j = new DateTime("it's today");
    }
}

class Other
{
    public function run()
    {
        return new \Other\DateTime('now');
    }
}
-----
<?php

namespace App;

use DateTime;
use DateTimeImmutable as Imm;

class More
{
    public function run($s)
    {
        $a = \Carbon\Carbon::now();
        $b = \Carbon\CarbonImmutable::now()->addHours(2)->subMinutes(3);
        $c = \Carbon\Carbon::yesterday()->addWeeks(1);
        $d = new DateTime($s);
        $e = new DateTime('now', null);
        $f = new \DateTimeZone('UTC');
        $g = \Carbon\Carbon::parse('next monday')->addDays(5);
        $h = (\Carbon\Carbon::tomorrow())->modify('+1 day');
        $i = \Carbon\Carbon::parse('s')->addSeconds(7);
        $j = \Carbon\Carbon::parse('it\'s today');
    }
}

class Other
{
    public function run()
    {
        return new \Other\DateTime('now');
    }
}
