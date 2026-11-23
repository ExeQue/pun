<?php

namespace App;

use Illuminate\Contracts\Container\Container;

function run(Container $app, $x)
{
    $app['a'] .= 'b';
    $app['a'] ??= 'b';
    $r = &$app['a'];
    isset($app['a'], $app['b']);
    $app[] = 'x';
    echo "{$app['a']}";
    $app['a']['b'] = 1;
    $y = $app['a']['b'];
    $app['c'] = &$x;
    sort($app['list']);
    return isset($app['a']);
}
-----
<?php

namespace App;

use Illuminate\Contracts\Container\Container;

function run(Container $app, $x)
{
    $app['a'] .= 'b';
    $app['a'] ??= 'b';
    $r = &$app['a'];
    isset($app['a'], $app['b']);
    $app[] = 'x';
    echo "{$app->make('a')}";
    $app['a']['b'] = 1;
    $y = $app->make('a')['b'];
    $app['c'] = &$x;
    sort($app['list']);
    return $app->bound('a');
}
