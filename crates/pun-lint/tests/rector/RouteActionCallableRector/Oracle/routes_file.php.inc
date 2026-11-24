<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::get('/a', 'SomeController@index');
Route::group(['namespace' => 'SomeOtherNamespace'], function () {
    Route::get('/b', 'SomeController@index');
});
-----
<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::get('/a', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeController::class, 'index']);
Route::group(['namespace' => 'SomeOtherNamespace'], function () {
    Route::get('/b', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeOtherNamespace\SomeController::class, 'index']);
});
