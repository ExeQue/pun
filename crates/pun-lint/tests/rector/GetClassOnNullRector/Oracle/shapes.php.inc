<?php

class Shapes
{
    public function run(?object $maybe, object $sure)
    {
        $a = get_class($maybe);
        $b = 'name: ' . get_class($maybe);
        $c = get_class($sure);
        $d = get_class(null);
        $e = $sure ? 1 : 2;
        $f = get_class($maybe);
    }
}
-----
<?php

class Shapes
{
    public function run(?object $maybe, object $sure)
    {
        $a = $maybe !== null ? get_class($maybe) : self::class;
        $b = 'name: ' . ($maybe !== null ? get_class($maybe) : self::class);
        $c = get_class($sure);
        $d = null !== null ? get_class(null) : self::class;
        $e = $sure ? 1 : 2;
        $f = get_class($maybe);
    }
}
