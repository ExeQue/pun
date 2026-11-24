<?php

namespace Pun\SwitchToMatch;

// Rector merges the two assigns by their property name and writes `$this->a = match ...`
function propertiesOfDifferentObjects($v, $that)
{
    switch ($v) {
        case 1:
            $this->a = 1;
            break;
        default:
            $that->a = 2;
    }
}
