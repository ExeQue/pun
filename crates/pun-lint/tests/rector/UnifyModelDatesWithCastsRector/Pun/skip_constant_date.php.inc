<?php

namespace App;

use Illuminate\Database\Eloquent\Model;

class Skip extends Model
{
    const DATE = 'seen_at';

    protected $dates = [self::DATE];
}
