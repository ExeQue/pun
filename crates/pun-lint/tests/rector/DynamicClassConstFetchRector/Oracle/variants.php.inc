<?php

namespace App;

class Example
{
    public function run($object, $name)
    {
        $a = constant(self::class . '::' . $name);
        $b = constant(static::class . "::" . $name);
        $c = constant($object::class . '::' . strtoupper($name));
        $d = \constant(Example::class . '::' . $this->name);
    }
}
-----
<?php

namespace App;

class Example
{
    public function run($object, $name)
    {
        $a = self::{$name};
        $b = static::{$name};
        $c = $object::{strtoupper($name)};
        $d = Example::{$this->name};
    }
}
