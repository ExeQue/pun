<?php

namespace App;

/**
 * @method static string magic()
 */
class Shapes
{
    public static function one()
    {
    }

    public function run()
    {
        $this->one();
        $this->ONE(1, 2);
        $this->magic();
        $this->other();
        $this->one(...);
        echo "{$this->one()}";
        $f = function () {
            return $this->one();
        };
        new class {
            public static function two()
            {
            }

            public function run()
            {
                $this->two();
                $this->one();
            }
        };
    }

    public function other()
    {
    }
}

final class FinalShapes
{
    public static function one()
    {
    }

    public function run()
    {
        return $this->one();
    }
}

trait Shared
{
    public static function one()
    {
    }

    public function run()
    {
        return $this->one();
    }
}

class UsesTrait
{
    use Shared;

    public function go()
    {
        return $this->one();
    }
}
-----
<?php

namespace App;

/**
 * @method static string magic()
 */
class Shapes
{
    public static function one()
    {
    }

    public function run()
    {
        static::one();
        static::ONE(1, 2);
        $this->magic();
        $this->other();
        $this->one(...);
        echo "{$this->one()}";
        $f = function () {
            return static::one();
        };
        new class {
            public static function two()
            {
            }

            public function run()
            {
                static::two();
                static::one();
            }
        };
    }

    public function other()
    {
    }
}

final class FinalShapes
{
    public static function one()
    {
    }

    public function run()
    {
        return self::one();
    }
}

trait Shared
{
    public static function one()
    {
    }

    public function run()
    {
        return $this->one();
    }
}

class UsesTrait
{
    use Shared;

    public function go()
    {
        return $this->one();
    }
}
