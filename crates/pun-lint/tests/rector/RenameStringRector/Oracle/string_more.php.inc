<?php

namespace App;

class More
{
    public function run()
    {
        $a = "ROLE_PREVIOUS_ADMIN";
        $b = 'role_previous_admin';
        $c = ['ROLE_PREVIOUS_ADMIN' => 'ROLE_PREVIOUS_ADMIN'];
        $d = "ROLE_PREVIOUS_ADMIN $x";
        $e = 'OLD_\\';
        return $a . 'OTHER';
    }
}
-----
<?php

namespace App;

class More
{
    public function run()
    {
        $a = 'IS_IMPERSONATOR';
        $b = 'role_previous_admin';
        $c = ['IS_IMPERSONATOR' => 'IS_IMPERSONATOR'];
        $d = "ROLE_PREVIOUS_ADMIN $x";
        $e = 'it\'s\\';
        return $a . 'OTHER';
    }
}
