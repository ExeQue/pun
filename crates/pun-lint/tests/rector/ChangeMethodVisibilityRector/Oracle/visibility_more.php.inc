<?php

namespace App;

use Lib\ParentObject;

class More extends ParentObject
{
    public static function makePrivate()
    {
    }

    final protected function makePublic()
    {
    }

    function makeStatic()
    {
    }

    static public function makePublic2()
    {
    }

    public function other()
    {
    }
}

class NotChild
{
    public function makePrivate()
    {
    }
}
-----
<?php

namespace App;

use Lib\ParentObject;

class More extends ParentObject
{
    private static function makePrivate()
    {
    }

    final public function makePublic()
    {
    }

    static function makeStatic()
    {
    }

    static public function makePublic2()
    {
    }

    public function other()
    {
    }
}

class NotChild
{
    public function makePrivate()
    {
    }
}
