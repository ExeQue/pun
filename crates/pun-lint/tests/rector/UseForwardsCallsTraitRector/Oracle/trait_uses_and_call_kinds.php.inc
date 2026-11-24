<?php

namespace App;

use Illuminate\Support\Traits\ForwardsCalls;

class Imported
{
    use ForwardsCalls;

    public function __call($method, $args)
    {
        return call_user_func_array([$this->foo, $method], $args);
    }
}

class OtherTrait
{
    use Macroable, Other;

    public function __call($method, $args)
    {
        return call_user_func_array([$this->foo, $method], $args);
    }
}

class StringTarget
{
    public static function __callStatic($method, $args)
    {
        return call_user_func_array(['Foo', $method], $args);
    }
}

class ThreeItems
{
    public function run()
    {
        return call_user_func([$this->foo, 'a', 'b']);
    }
}

class Nested
{
    public function run()
    {
        return function () {
            return call_user_func([$this->foo, 'bar'], 1, 2);
        };
    }

    public function other()
    {
        return call_user_func_array([$this->foo, 'bar']);
    }
}

class FirstClass
{
    public function run($args)
    {
        return call_user_func($this->foo->bar(...), $args, 2);
    }
}
-----
<?php

namespace App;

use Illuminate\Support\Traits\ForwardsCalls;

class Imported
{
    use ForwardsCalls;

    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
}

class OtherTrait
{
    use Macroable, Other, \Illuminate\Support\Traits\ForwardsCalls;

    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
}

class StringTarget
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public static function __callStatic($method, $args)
    {
        return $this->forwardCallTo('Foo', $method, $args);
    }
}

class ThreeItems
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function run()
    {
        return call_user_func([$this->foo, 'a', 'b']);
    }
}

class Nested
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function run()
    {
        return function () {
            return $this->forwardCallTo($this->foo, 'bar', [1, 2]);
        };
    }

    public function other()
    {
        return $this->forwardCallTo($this->foo, 'bar', []);
    }
}

class FirstClass
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function run($args)
    {
        return $this->forwardCallTo($this->foo, 'bar', [$args, 2]);
    }
}
