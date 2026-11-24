<?php

namespace App;

class Doc
{
    /**
     * The target.
     */
    public function __call($method, $args)
    {
        return call_user_func_array([$this->foo, $method], $args);
    }
}

class Comment
{
    // forward
    public function __call($method, $args)
    {
        return call_user_func_array([$this->foo, $method], $args);
    }
}

$x = new class {
    public function __call($method, $args)
    {
        return call_user_func_array([$this->foo, $method], $args);
    }
};

class Keyed
{
    public function __call($method, $args)
    {
        return call_user_func_array(['a' => $this->foo, 'b' => $method], $args);
    }
}
-----
<?php

namespace App;

class Doc
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    /**
     * The target.
     */
    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
}

class Comment
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    // forward
    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
}

$x = new class {
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
};

class Keyed
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function __call($method, $args)
    {
        return $this->forwardCallTo($this->foo, $method, $args);
    }
}
