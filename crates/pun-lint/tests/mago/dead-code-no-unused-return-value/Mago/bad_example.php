<?php

final class Counter
{
    private int $count = 0;

    public function increment(): void
    {
        $this->advance();
    }

    private function advance(): int
    {
        return ++$this->count;
    }
}
