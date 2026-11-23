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
            return static::class;
        };
        $b = fn () => static::class;

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
        return fn () => static::class;
    }
}

function outside()
{
    return get_called_class();
}
