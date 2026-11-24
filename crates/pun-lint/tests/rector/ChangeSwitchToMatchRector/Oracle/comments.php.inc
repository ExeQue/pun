<?php

namespace Pun\Oracle\SwitchToMatch;

function nopAfterBreak($v)
{
    switch ($v) {
        case 1:
            $a = 1;
            break;
        // comment
        case 2:
            $a = 2;
            break;
        default:
            $a = 3;
            // trailing
    }
    return $a;
}

function trailingComment($v)
{
    switch ($v) {
        case 1: // one
            $a = 1; // note
            break;
        default:
            /*
             * multi
             */
            $a = 3;
    }
    return $a;
}

function throwNextComment($v)
{
    switch ($v) {
        case 1:
            return 1;
    }

    // why
    throw new E();
}

function nopNext($v)
{
    switch ($v) {
        case 1:
            return 1;
        case 2:
            return 2;
    };
    return 3;
}

function returnAssignNext($v)
{
    switch ($v) {
        case 1:
            $a = 1;
            break;
        case 2:
            $a = 2;
            break;
    }
    // keep
    return $a;
}
-----
<?php

namespace Pun\Oracle\SwitchToMatch;

function nopAfterBreak($v)
{
    $a = match ($v) {
        1 => 1,
        2 => 2,
        default => 3,
    };
    return $a;
}

function trailingComment($v)
{
    $a = match ($v) {
        // one
        1 => 1,
        /*
         * multi
         */
        default => 3,
    };
    return $a;
}

function throwNextComment($v)
{
    return match ($v) {
        1 => 1,
        default => throw new E(),
    };
}

function nopNext($v)
{
    return match ($v) {
        1 => 1,
        2 => 2,
        default => 3,
    };
}

function returnAssignNext($v)
{
    $a = match ($v) {
        1 => 1,
        2 => 2,
        // keep
        default => $a,
    };
    // keep
    return $a;
}
