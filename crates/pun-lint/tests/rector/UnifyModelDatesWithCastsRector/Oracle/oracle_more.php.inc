<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

class Post extends Model
{
    protected $casts = [];

    protected $dates = ['published_at', 'archived_at'];
}

class Comment extends Model
{
    protected $casts = ['seen_at' => 'date'];

    protected $dates = ['seen_at', 'edited_at'];
}
-----
<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

class Post extends Model
{
    protected $casts = ['published_at' => 'datetime', 'archived_at' => 'datetime'];
}

class Comment extends Model
{
    protected $casts = ['seen_at' => 'date', 'edited_at' => 'datetime'];
}
