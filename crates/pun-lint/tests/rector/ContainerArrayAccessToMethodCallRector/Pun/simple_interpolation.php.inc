<?php

namespace App;

use Illuminate\Contracts\Container\Container;

function run(Container $app)
{
    echo "{$app['a']} and $app[b]";
    echo <<<TXT
    {$app['c']}
    TXT;
}
-----
<?php

namespace App;

use Illuminate\Contracts\Container\Container;

function run(Container $app)
{
    echo "{$app->make('a')} and $app[b]";
    echo <<<TXT
    {$app->make('c')}
    TXT;
}
