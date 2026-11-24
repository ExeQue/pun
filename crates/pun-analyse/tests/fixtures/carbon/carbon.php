<?php

// nesbot/carbon 3.15.1's declarations the probe reaches, with their signatures, without their bodies.

namespace Carbon;

interface CarbonInterface extends \DateTimeInterface
{
    public function addDays($value = 1);
}

class Carbon extends \DateTime implements CarbonInterface
{
    /**
     * @param-closure-this static $macro
     */
    public static function macro(string $name, ?callable $macro): void {}

    public function addDays($value = 1) {}
}

class CarbonImmutable extends \DateTimeImmutable implements CarbonInterface
{
    /**
     * @param-closure-this static $macro
     */
    public static function macro(string $name, ?callable $macro): void {}

    public static function parse($time = null, $timezone = null): static {}

    public function addDays($value = 1) {}
}

class CarbonPeriod extends DatePeriodBase
{
    public static function create(...$params): static {}
}
