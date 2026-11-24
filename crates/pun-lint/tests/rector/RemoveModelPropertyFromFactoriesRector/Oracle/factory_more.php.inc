<?php

namespace App;

use Illuminate\Database\Eloquent\Factories\Factory;

class AFactory extends Factory
{
    /**
     * The model.
     */
    protected $model = Foo::class;

    public function definition()
    {
        return [];
    }
}

class CFactory extends Factory
{
    public function definition()
    {
        return [];
    }

    protected $model = Foo::class;
}

class NotFactory
{
    protected $model = Foo::class;
}

class DFactory extends AFactory
{
    protected static $model = Foo::class;
}
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Factories\Factory;

class AFactory extends Factory
{
    public function definition()
    {
        return [];
    }
}

class CFactory extends Factory
{
    public function definition()
    {
        return [];
    }
}

class NotFactory
{
    protected $model = Foo::class;
}

class DFactory extends AFactory
{
}
