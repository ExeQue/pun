<?php

final class FinalWithAnonymous
{
    public function run()
    {
        $fn = function () {
            return $this::class;
        };

        return new class {
            public function name()
            {
                return $this::CLASS;
            }
        };
    }
}

enum Suit
{
    public function name()
    {
        return $this::class;
    }
}

trait WithAnonymous
{
    public function run()
    {
        return new class {
            public function name()
            {
                return $this::class;
            }
        };
    }
}
-----
<?php

final class FinalWithAnonymous
{
    public function run()
    {
        $fn = function () {
            return self::class;
        };

        return new class {
            public function name()
            {
                return self::CLASS;
            }
        };
    }
}

enum Suit
{
    public function name()
    {
        return $this::class;
    }
}

trait WithAnonymous
{
    public function run()
    {
        return new class {
            public function name()
            {
                return static::class;
            }
        };
    }
}
