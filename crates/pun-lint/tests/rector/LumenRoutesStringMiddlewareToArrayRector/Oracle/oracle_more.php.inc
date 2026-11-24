<?php

namespace App;

/** @var \Laravel\Lumen\Routing\Router $router */
$router->post('/', ['middleware' => 'a|b|c', 'uses' => 'C@x']);
$router->group(['prefix' => 'x', 'middleware' => 'auth'], function () {});
$router->head('/', ['middleware' => 'auth']);
-----
<?php

namespace App;

/** @var \Laravel\Lumen\Routing\Router $router */
$router->post('/', ['middleware' => ['a', 'b', 'c'], 'uses' => 'C@x']);
$router->group(['prefix' => 'x', 'middleware' => ['auth']], function () {});
$router->head('/', ['middleware' => 'auth']);
