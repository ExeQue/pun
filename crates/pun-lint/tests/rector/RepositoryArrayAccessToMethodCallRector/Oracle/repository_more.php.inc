<?php

namespace App;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Contracts\Cache\Repository as Cache;

function run(Repository $config, Cache $cache)
{
    unset($config['a'], $config['b']);
    unset($cache['a']);
    $config['x'] = $cache['y'];
    $config['a'] .= 'b';
}
-----
<?php

namespace App;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Contracts\Cache\Repository as Cache;

function run(Repository $config, Cache $cache)
{
    unset($config['a'], $config['b']);
    $cache->forget('a');
    $config->set('x', $cache->get('y'));
    $config['a'] .= 'b';
}
