<?php

namespace App;

use Faker\Generator;

function make(Generator $faker)
{
    $name = $faker->name;
    $faker->unique()->email;
    return $faker->address . $faker->city;
}
-----
<?php

namespace App;

use Faker\Generator;

function make(Generator $faker)
{
    $name = $faker->name();
    $faker->unique()->email;
    return $faker->address() . $faker->city();
}
