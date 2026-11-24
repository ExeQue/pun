<?php

use App\User;

$factory->define(User::class, function ($faker) {
    return ['a' => 1];
});

$factory->state(User::class, 'x', ['c' => 3]);
-----
<?php

use App\User;
class UserFactory extends \Illuminate\Database\Eloquent\Factories\Factory
{
    protected $model = User::class;
    public function definition()
    {
        return ['a' => 1];
    }
    public function x()
    {
        return $this->state(['c' => 3]);
    }
}
