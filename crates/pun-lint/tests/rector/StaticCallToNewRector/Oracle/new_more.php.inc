<?php

namespace App;

use Lib\Response as R;

class More
{
    public function run()
    {
        $a = R::create();
        $b = \Lib\Response::CREATE(1, 2);
        $c = R::create()->send();
        $d = R::other();
        $e = \Lib\Other::create();
    }
}
-----
<?php

namespace App;

use Lib\Response as R;

class More
{
    public function run()
    {
        $a = new \Lib\Response();
        $b = new \Lib\Response(1, 2);
        $c = (new \Lib\Response())->send();
        $d = R::other();
        $e = \Lib\Other::create();
    }
}
