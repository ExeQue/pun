<?php

final class CallSites
{
    public function run(Other $other): void
    {
        $this->anyReceiver();
        $other->anyReceiver();
        Other::anyReceiver();

        $this->ADVANCE();

        $this->nested($this->inner());
        $this->inner();

        $this->chained()->done();

        $this->voidCast();
        (void) $this->voidCast();

        $this->coalesced();
        $this->coalesced() ?? null;

        $this->returned();
        $closure = function () {
            $this->inClosure();
            return $this->returned();
        };

        $this->notCalled(...);

        parent::fromParent();
        static::fromStatic();
    }

    public function hook(): int
    {
        $this->onlyInHook();

        return 1;
    }

    private function anyReceiver(): int { return 1; }
    private function advance(): int { return 1; }
    private function nested(int $value): ?int { return $value; }
    private function inner(): int { return 1; }
    private function chained(): self { return $this; }
    private function done(): static { return $this; }
    private function voidCast(): int { return 1; }
    private function coalesced(): ?int { return 1; }
    private function returned(): int { return 1; }
    private function inClosure(): int|string { return 1; }
    private function notCalled(): int { return 1; }
    private function fromParent(): mixed { return 1; }
    private static function fromStatic(): null { return null; }
}
