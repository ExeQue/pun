<?php

namespace App\Props;

class Model
{
    public $oldName;
    public static $count = 0;
    public $kept, $other;
    public $taken;

    public function run(?Model $maybe, $any)
    {
        $this->oldName = 1;
        $this->OLDNAME = 2;
        self::$count++;
        static::$count--;
        Model::$count = 3;
        $maybe->oldName = 4;
        $maybe?->oldName;
        $any->oldName = 5;
        $this->kept = 6;
        $fn = fn() => $this->oldName;
    }
}

class Child extends Model
{
    public $oldName = 'child';
}

class Unrelated
{
    public $oldName;

    public function run()
    {
        return $this->oldName;
    }
}
-----
<?php

namespace App\Props;

class Model
{
    public $newName;
    public static $total = 0;
    public $kept, $other;
    public $taken;

    public function run(?Model $maybe, $any)
    {
        $this->newName = 1;
        $this->newName = 2;
        self::$total++;
        static::$total--;
        Model::$total = 3;
        $maybe->newName = 4;
        $maybe?->oldName;
        $any->oldName = 5;
        $this->taken = 6;
        $fn = fn() => $this->newName;
    }
}

class Child extends Model
{
    public $newName = 'child';
}

class Unrelated
{
    public $oldName;

    public function run()
    {
        return $this->oldName;
    }
}
