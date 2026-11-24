<?php

namespace App;

use Illuminate\Database\Schema\Blueprint;

function up(Blueprint $table)
{
    $table->point('location', 4326);
    $table->multiPolygon('area');
    $table->geometry('shape');
}
-----
<?php

namespace App;

use Illuminate\Database\Schema\Blueprint;

function up(Blueprint $table)
{
    $table->geometry('location', 4326, 'point');
    $table->geometry('area', 'multiPolygon');
    $table->geometry('shape');
}
