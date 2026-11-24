<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::prefix('x')->group(function () {
    Route::get('/l', 'MoreController@index');
});
Route::group(['middleware' => 'auth'], function () {
    Route::get('/m', 'MoreController@index');
});
Route::group(['namespace' => 'SomeNamespace'], function () {
    Route::group(['middleware' => 'auth'], function () {
        Route::get('/n', 'SomeController@index');
    });
    Route::group(['namespace' => 'SomeOtherNamespace'], fn () => Route::get('/f', 'SomeController@index'));
});
Route::group(['prefix' => 'x'], function () {
    Route::get('/g', 'MoreController@index');
});
Route::group(['namespace' => 'SomeNamespace', 'namespace' => 'Other'], function () {
    Route::get('/h', 'SomeController@index');
});
Route::group(array('namespace' => "SomeNamespace"), function () {
    Route::get('/i', 'SomeController@index');
});
Route::group(['namespace' => 'SomeNamespace'], function () {
    Route::get('/j', 'SomeController@index');
}, 'SomeController@index');
Route::group(['namespace' => $x], function () {
    Route::get('/k', 'SomeController@index');
    Route::group(['namespace' => 'SomeNamespace'], function () {
        Route::get('/l', 'SomeController@index');
    });
});
-----
<?php

namespace App;

use Illuminate\Support\Facades\Route;

Route::prefix('x')->group(function () {
    Route::get('/l', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
});
Route::group(['middleware' => 'auth'], function () {
    Route::get('/m', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
});
Route::group(['namespace' => 'SomeNamespace'], function () {
    Route::group(['middleware' => 'auth'], function () {
        Route::get('/n', 'SomeController@index');
    });
    Route::group(['namespace' => 'SomeOtherNamespace'], fn () => Route::get('/f', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeOtherNamespace\SomeController::class, 'index']));
});
Route::group(['prefix' => 'x'], function () {
    Route::get('/g', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
});
Route::group(['namespace' => 'SomeNamespace', 'namespace' => 'Other'], function () {
    Route::get('/h', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeController::class, 'index']);
});
Route::group(array('namespace' => "SomeNamespace"), function () {
    Route::get('/i', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeController::class, 'index']);
});
Route::group(['namespace' => 'SomeNamespace'], function () {
    Route::get('/j', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\SomeNamespace\SomeController::class, 'index']);
}, 'SomeController@index');
Route::group(['namespace' => $x], function () {
    Route::get('/k', 'SomeController@index');
    Route::group(['namespace' => 'SomeNamespace'], function () {
        Route::get('/l', 'SomeController@index');
    });
});
