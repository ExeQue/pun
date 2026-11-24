<?php

namespace App;

use Illuminate\Support\Facades\Route;

class Routes
{
    public function run()
    {
        Route::redirect('/a', '/b', 301);
        Route::redirect('/a', '/b', 302);
        Route::redirect('/a', '/b');
        \Some\Other::redirect('/a', '/b', 301);
    }
}
-----
<?php

namespace App;

use Illuminate\Support\Facades\Route;

class Routes
{
    public function run()
    {
        Route::permanentRedirect('/a', '/b');
        Route::redirect('/a', '/b', 302);
        Route::redirect('/a', '/b');
        \Some\Other::redirect('/a', '/b', 301);
    }
}
