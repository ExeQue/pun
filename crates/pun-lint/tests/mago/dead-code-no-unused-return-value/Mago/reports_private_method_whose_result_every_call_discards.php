<?php

final class Counter
{
    private int $count = 0;

    public function increment(): void
    {
        $this->advance();
        ($this->advance());
        self::reset();
    }

    public function getCount(): int
    {
        return $this->count;
    }

    private function advance(): int
    {
        return ++$this->count;
    }

    private static function reset(): bool
    {
        return true;
    }
}
