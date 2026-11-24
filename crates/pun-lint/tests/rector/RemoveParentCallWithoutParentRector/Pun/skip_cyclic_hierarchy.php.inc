<?php

namespace Pun\Tests\RemoveParentCallWithoutParent\Cyclic;

class First extends Second
{
    public function run()
    {
        parent::run();
    }
}

class Second extends First
{
}
