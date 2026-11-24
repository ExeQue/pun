<?php

class Shapes
{
    public function run()
    {
        $a = \ReflectionFunction::export('foo');
        $b = \ReflectionFunction::export('foo', true) . 'x';
        $c = \ReflectionFunction::export('foo', false);
        $d = \ReflectionClass::export('foo', true);
        $e = \ReflectionMethod::EXPORT('foo', 'bar');
        $f = \ReflectionFunction::export();
        $g = \DateTime::export('foo');
        echo \ReflectionFunction::export('foo', true);
    }
}
-----
<?php

class Shapes
{
    public function run()
    {
        $a = new \ReflectionFunction('foo');
        $b = (string) new \ReflectionFunction('foo') . 'x';
        $c = new \ReflectionFunction('foo');
        $d = \ReflectionClass::export('foo', true);
        $e = \ReflectionMethod::EXPORT('foo', 'bar');
        $f = \ReflectionFunction::export();
        $g = \DateTime::export('foo');
        echo (string) new \ReflectionFunction('foo');
    }
}
