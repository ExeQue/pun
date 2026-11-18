<?php

namespace App;

final class NullChecks
{
    public function notNull()
    {
        $newNode = $this->bar();
        if ($newNode !== null) {
            return $newNode;
        }

        return null;
    }

    public function nullable()
    {
        return $this->bar();
    }

    private function bar(): ?\stdClass
    {
        if (rand(0, 1)) {
            return new \stdClass;
        }

        return null;
    }
}
