<?php

namespace App;

use Carbon\Carbon;
use Carbon\CarbonImmutable;
use Carbon\CarbonInterface;
use Carbon\CarbonPeriod;
use Carbon\DatePeriodBase;

final class Probe
{
    public function instanceMacro(Carbon $date): CarbonInterface
    {
        $date->addBusinessDays();
        $date->addBusinessDays('three');

        return $date->addBusinessDays(3);
    }

    public function onEveryCarbon(CarbonImmutable $date, CarbonInterface $any, MyDate $mine): CarbonInterface
    {
        $any->addBusinessDays(1, 2);
        $mine->addBusinessDays('x');

        return $date->addBusinessDays(1);
    }

    public function staticMacro(): CarbonImmutable
    {
        Carbon::addBusinessDays(2);
        Carbon::fromTicket(42);

        return CarbonImmutable::fromTicket('2026-11-24');
    }

    public function methodMacro(Carbon $date): string
    {
        $date->quarterName('one');
        Carbon::quarterName(2)->format('Y');

        return $date->quarterName(1);
    }

    public function functionMacro(Carbon $date): bool
    {
        $date->isPayday('25');

        return $date->isPayday() && $date->isPayday(24);
    }

    public function unknown(Carbon $date): void
    {
        $date->notAMacro();
        Carbon::notAStaticMacro();
    }

    public function period(): \DatePeriod
    {
        $period = CarbonPeriod::create('2026-01-01', '2026-01-10');
        $base = $period instanceof DatePeriodBase;

        return $period;
    }
}
