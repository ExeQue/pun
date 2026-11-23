<?php

class Open
{
    public function run()
    {
        $a = function () {
            return get_called_class();
        };
        $b = fn () => GET_CALLED_CLASS();

        return new class {
            public function name()
            {
                return \get_called_class();
            }
        };
    }
}

enum Suit
{
    public function name()
    {
        return get_called_class();
    }
}

final class Closed
{
    public static function make()
    {
        return [get_called_class(), get_called_class(1)];
    }
}

trait Shared
{
    public function name()
    {
        return fn () => get_called_class();
    }
}

function outside()
{
    return get_called_class();
}
-----
<?php

class Open
{
    public function run()
    {
        $a = function () {
            return self::class;
        };
        $b = fn () => self::class;

        return new class {
            public function name()
            {
                return self::class;
            }
        };
    }
}

enum Suit
{
    public function name()
    {
        return self::class;
    }
}

final class Closed
{
    public static function make()
    {
        return [self::class, self::class];
    }
}

trait Shared
{
    public function name()
    {
        return fn () => self::class;
    }
}

function outside()
{
    return get_called_class();
}
