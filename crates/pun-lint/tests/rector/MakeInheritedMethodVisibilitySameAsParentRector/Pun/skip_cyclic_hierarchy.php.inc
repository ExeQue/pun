<?php

namespace Pun\Tests\MakeInheritedMethodVisibilitySameAsParent\Cyclic;

class First extends Second
{
    protected function run()
    {
    }
}

class Second extends First
{
    public function run()
    {
    }
}
