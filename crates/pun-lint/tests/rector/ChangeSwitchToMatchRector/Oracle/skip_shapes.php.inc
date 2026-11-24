<?php

namespace Pun\Oracle\SwitchToMatch;

function numeric()
{
    $h = ini_get('x') ?: 5;
    switch ($h) {
        case 4:
            $p = 'a';
            break;
        case 5:
        default:
            $p = 'b';
            break;
    }
    return $p;
}

function mixedTypes($v)
{
    switch ($v) {
        case 4:
            $p = 'a';
            break;
        case 'x':
            $p = 'c';
            break;
        default:
            $p = 'b';
            break;
    }
    return $p;
}

function twoVariables($v)
{
    switch ($v) {
        case 1:
            $p = 'a';
            break;
        case 2:
            $q = 'a';
            break;
        default:
            $p = 'b';
    }
    return $p;
}

function echoCase($v)
{
    switch ($v) {
        case 1:
            echo 'a';
            break;
        default:
            echo 'b';
    }
}

function throwOnly($v)
{
    switch ($v) {
        case 1:
            throw new A();
        default:
            throw new B();
    }
}

function assignThrowNext($v)
{
    switch ($v) {
        case 1:
            $a = 1;
            break;
    }
    throw new E();
}

function intCond(int $v)
{
    switch ($v) {
        case '1':
            return 'a';
        case 2:
            return 'c';
        default:
            return 'b';
    }
}

function emptyReturn($v)
{
    switch ($v) {
        case 1:
            return;
        default:
            return 2;
    }
}

function returnAndAssign($v)
{
    switch ($v) {
        case 1:
            return 1;
        default:
            $a = 2;
    }
    return $a;
}
