<?php

namespace App;

class NestedCall
{
    public function run($x)
    {
        return call_user_func([$this->a, 'b'], call_user_func(array($this->c, 'd'), $x));
    }
}
-----
<?php

namespace App;

class NestedCall
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function run($x)
    {
        return $this->forwardCallTo($this->a, 'b', [$this->forwardCallTo($this->c, 'd', [$x])]);
    }
}
