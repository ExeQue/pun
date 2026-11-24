<?php

namespace App;

use App\User;

$factory->define(User::class, function ($faker) {
    return ['a' => 1];
});

$factory->define(Account::class, function ($faker) {
    return ['b' => 2];
});

$factory->state(User::class, 'x', ['c' => 3]);
-----
<?php

namespace App;

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
class AccountFactory extends \Illuminate\Database\Eloquent\Factories\Factory
{
    protected $model = Account::class;
    public function definition()
    {
        return ['b' => 2];
    }
}
