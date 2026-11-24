<?php

namespace App\Named;

class Box
{
    public function put($item, $strict = false) {}

    public function run(?Box $other, Box $box, Box|\ArrayObject $either)
    {
        $other->put(1, true);
        $other?->put(1, true);
        $box->put(1, true);
        $box?->put(1, true);
        $either->put(1, true);
        (new Box())->put(1, true);
    }
}
-----
<?php

namespace App\Named;

class Box
{
    public function put($item, $strict = false) {}

    public function run(?Box $other, Box $box, Box|\ArrayObject $either)
    {
        $other->put(1, true);
        $other?->put(1, true);
        $box->put(1, strict: true);
        $box?->put(1, true);
        $either->put(1, true);
        (new Box())->put(1, strict: true);
    }
}
