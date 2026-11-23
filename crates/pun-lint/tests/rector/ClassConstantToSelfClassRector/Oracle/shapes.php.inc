<?php

class A
{
    public function name()
    {
        return [__CLASS__, __class__, fn () => __CLASS__, new class {
            public $name = __CLASS__;
        }];
    }
}

trait T
{
    public function name()
    {
        return __CLASS__;
    }
}
-----
<?php

class A
{
    public function name()
    {
        return [self::class, self::class, fn () => self::class, new class {
            public $name = self::class;
        }];
    }
}

trait T
{
    public function name()
    {
        return self::class;
    }
}
