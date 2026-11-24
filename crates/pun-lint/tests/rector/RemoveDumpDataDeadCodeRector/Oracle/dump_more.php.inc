<?php

namespace App;

class C
{
    public function a($x)
    {
        dd($x);
        \dump($x, 1);
        $y = dump($x);
        if ($x) dd($x);
        // keep
        dump($x); // trailing
        $x->dump();
        Dump($x);
        return $y;
    }
}
-----
<?php

namespace App;

class C
{
    public function a($x)
    {
        $y = dump($x);
        if ($x) {
        } // trailing
        $x->dump();
        return $y;
    }
}
