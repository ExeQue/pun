<?php

namespace App;

final class EmptyBlocks
{
    public function ifForeachFor(array $items): void
    {
        if ($items) {
        }
    }

    public function loop(int $i): void
    {
    }
}
