<?php

namespace App;

final class Coalesce
{
    protected ?\stdClass $prop1 = null;

    protected ?\stdClass $prop2 = null;

    public function arrayKeyExists($values, $keyToMatch)
    {
        return $values[$keyToMatch] ?? null;
    }

    public function consecutiveReturns(): \stdClass
    {
        if ($this->prop1 !== null) {
            return $this->prop1;
        }

        if ($this->prop2 !== null) {
            return $this->prop2;
        }

        return new \stdClass();
    }

    public function foreach($currentFunction)
    {
        $oldToNewFunctions = ['a' => 'b'];
        foreach ($oldToNewFunctions as $oldFunction => $newFunction) {
            if ($oldFunction === $currentFunction) {
                return $newFunction;
            }
        }

        return 45;
    }
}
