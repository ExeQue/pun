<?php

use App\Helpers;
use Carbon\Carbon;
use Carbon\CarbonImmutable;
use Carbon\CarbonInterface;

function is_payday(int $day = 25): bool
{
    return $day === 25;
}

Carbon::macro('addBusinessDays', function (int $days): CarbonInterface {
    return $this->addDays($days);
});

CarbonImmutable::macro('fromTicket', static function (string $ticket): CarbonImmutable {
    return CarbonImmutable::parse($ticket);
});

Carbon::macro('quarterName', [Helpers::class, 'quarterName']);

Carbon::macro('isPayday', 'is_payday');
