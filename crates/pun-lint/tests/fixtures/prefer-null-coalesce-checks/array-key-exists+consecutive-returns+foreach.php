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
        return ($this->prop1 ?? $this->prop2) ?? new \stdClass();
    }

    public function foreach($currentFunction)
    {
        $oldToNewFunctions = ['a' => 'b'];
        return $oldToNewFunctions[$currentFunction] ?? 45;
    }
}
