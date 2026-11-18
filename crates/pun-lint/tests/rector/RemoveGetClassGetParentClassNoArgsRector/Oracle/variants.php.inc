<?php

namespace App;

class Child extends Base
{
    public function run()
    {
        return \get_class() . ' of ' . GET_PARENT_CLASS();
    }
}
-----
<?php

namespace App;

class Child extends Base
{
    public function run()
    {
        return self::class . ' of ' . parent::class;
    }
}
