<?php

namespace App;

class More extends \Lib\Base
{
    public function getItem()
    {
        if ($x) {
            return 2;
        }
        $f = function () {
            return 3;
        };
        return array(1);
    }

    public function GETITEM2()
    {
        return $this->x;
    }

    public function other()
    {
        return 1;
    }
}

class NotChild
{
    public function getItem()
    {
        return 1;
    }
}
-----
<?php

namespace App;

class More extends \Lib\Base
{
    public function getItem()
    {
        if ($x) {
            return 2;
        }
        $f = function () {
            return 3;
        };
        return array(1);
    }

    public function GETITEM2()
    {
        return [$this->x];
    }

    public function other()
    {
        return 1;
    }
}

class NotChild
{
    public function getItem()
    {
        return 1;
    }
}
