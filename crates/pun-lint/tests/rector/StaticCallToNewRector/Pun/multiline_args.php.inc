<?php

namespace App;

use Lib\Response as R;

class More
{
    public function run()
    {
        return R::create(
            1
        );
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
        return new \Lib\Response(
            1
        );
    }
}
