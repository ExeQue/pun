<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

class Quoted extends Model
{
    public function getRouteKeyName()
    {
        return "slug";
    }
}

class Interpolated extends Model
{
    public function getRouteKeyName(): string
    {
        return "slug{$this->id}";
    }
}

class Yielding extends Model
{
    public function getRouteKeyName()
    {
        yield 'slug';
        return 'slug';
    }
}

class Cased extends Model
{
    protected $table = 'x';

    public function GETROUTEKEYNAME(): string
    {
        return 'uuid';
    }
}
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

#[\Illuminate\Database\Eloquent\Attributes\RouteKey("slug")]
class Quoted extends Model
{
}

class Interpolated extends Model
{
    public function getRouteKeyName(): string
    {
        return "slug{$this->id}";
    }
}

class Yielding extends Model
{
    public function getRouteKeyName()
    {
        yield 'slug';
        return 'slug';
    }
}

#[\Illuminate\Database\Eloquent\Attributes\RouteKey('uuid')]
class Cased extends Model
{
    protected $table = 'x';
}
