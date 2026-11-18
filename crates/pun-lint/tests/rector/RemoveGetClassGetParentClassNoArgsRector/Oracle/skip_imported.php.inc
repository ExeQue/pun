<?php

namespace App;

use function Other\get_class;

class Child extends Base
{
    public function run()
    {
        return get_class() . get_called_class();
    }
}
