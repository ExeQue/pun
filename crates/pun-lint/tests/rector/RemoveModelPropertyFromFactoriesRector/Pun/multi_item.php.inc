<?php

namespace App;

use Illuminate\Database\Eloquent\Factories\Factory;

// Rector removes the whole statement, `$more` with it; Pun leaves it alone.
class BFactory extends Factory
{
    protected $other = 1;
    protected $model = Foo::class, $more = 2;
}
