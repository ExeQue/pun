<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::get('/a', '\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeController@index');
-----
<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::get('/a', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeController::class, 'index']);
