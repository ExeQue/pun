<?php

namespace App;

class Dynamic
{
    public function run($m)
    {
        return call_user_func($this->{$m}(...));
    }
}
-----
<?php

namespace App;

class Dynamic
{
    use \Illuminate\Support\Traits\ForwardsCalls;
    public function run($m)
    {
        return call_user_func($this->{$m}(...));
    }
}
