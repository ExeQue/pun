<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

#[\Foo]
class Legacy extends Model
{
    protected $guarded = array();
}

class Typed extends Model
{
    protected array $guarded = [];

    public function x()
    {
    }
}

$model = new class extends Model {
    protected $guarded = [];
};
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

#[\Foo]
#[\Illuminate\Database\Eloquent\Attributes\Unguarded]
class Legacy extends Model
{
}

#[\Illuminate\Database\Eloquent\Attributes\Unguarded]
class Typed extends Model
{
    public function x()
    {
    }
}

$model = new #[\Illuminate\Database\Eloquent\Attributes\Unguarded]
class extends Model
{
};
