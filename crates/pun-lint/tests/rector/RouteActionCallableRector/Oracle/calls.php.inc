<?php

namespace App;

use Illuminate\Routing\Router;
use Illuminate\Support\Facades\Route;

function routes(Router $router, $name)
{
    $router->get('/a', 'MoreController@index');
    Route::get('/c', 'MoreController@magic');
    Route::get('/d', 'MoreController@inherited');
    Route::GET('/e', 'MoreController@index');
    Route::get('/f', "MoreController@index");
    Route::get('/g', ['uses' => 'MoreController@index', 'as' => 1]);
    Route::get('/h', ['uses' => 'MoreController@index', 'middleware' => ['auth', 1]]);
    Route::get('/i', ['uses' => 'MoreController@index', 'domain' => 'x']);
    Route::get('/j', ['as' => 'j']);
    Route::get('/k', 'MoreController@');
    Route::get('/o', 'MoreController@index')->name('o');
    Route::get(uri: '/p', action: 'MoreController@index');
    Route::get('/q', 'More' . 'Controller@index');
    Route::get('/r', ['uses' => 'MoreController@index', 'as' => "it's", 'middleware' => []]);
    \Route::get('/s', 'MoreController@index');
    Route::get('/t', 'morecontroller@INDEX');
    Route::get('/u', ['uses' => 'MoreController@index', 'as' => $name]);
    Route::get('/v', ['uses' => 'MoreController@index', 'as' => true]);
    Route::match(['GET'], '/w', 'MoreController@index');
    Route::fallback(action: 'MoreController@index');
    Route::get('/x', ...['MoreController@index']);
    Route::get('/y', array('uses' => 'MoreController@index', 'as' => 'y'));
    Route::get('/z', ['uses' => 'MoreController@index', 'as' => 'z', 'middleware' => ['a', 'b' => 'c']]);
    Route::get('/aa', 'MoreController' . '@' . 'index');
    Route::get('/ab', ['uses' => 'MoreController@index', 'middleware' => 5]);
    Route::get('/ac', 'MoreController@index', 'extra');
    Route::get('/ad', 'MoreController@index@x');
    Route::get('/ae', 'MoreController');
    Route::redirect('/af', 'MoreController@index');
}
-----
<?php

namespace App;

use Illuminate\Routing\Router;
use Illuminate\Support\Facades\Route;

function routes(Router $router, $name)
{
    $router->get('/a', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/c', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'magic']);
    Route::get('/d', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'inherited']);
    Route::GET('/e', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/f', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/g', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/h', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/i', ['uses' => 'MoreController@index', 'domain' => 'x']);
    Route::get('/j', ['as' => 'j']);
    Route::get('/k', 'MoreController@');
    Route::get('/o', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index'])->name('o');
    Route::get(uri: '/p', action: [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/q', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/r', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index'])->name('it\'s')->middleware([]);
    \Route::get('/s', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/t', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\morecontroller::class, 'INDEX']);
    Route::get('/u', ['uses' => 'MoreController@index', 'as' => $name]);
    Route::get('/v', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::match(['GET'], '/w', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::fallback(action: [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/x', ...['MoreController@index']);
    Route::get('/y', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index'])->name('y');
    Route::get('/z', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index'])->name('z')->middleware(['a', 'c']);
    Route::get('/aa', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/ab', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index']);
    Route::get('/ac', [\RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source\MoreController::class, 'index'], 'extra');
    Route::get('/ad', 'MoreController@index@x');
    Route::get('/ae', 'MoreController');
    Route::redirect('/af', 'MoreController@index');
}
