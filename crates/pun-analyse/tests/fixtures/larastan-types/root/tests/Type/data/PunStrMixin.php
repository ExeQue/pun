<?php

namespace PunMixinMacros;

class PunStrMixin
{
    public function punShout(): \Closure
    {
        return function (string $value): string {
            return strtoupper($value).'!';
        };
    }

    public function punWhisper()
    {
        return fn (string $value): int => strlen($value);
    }

    protected function punHidden(): \Closure
    {
        return fn (): bool => true;
    }

    private function punPrivate(): \Closure
    {
        return fn (): float => 1.0;
    }
}
