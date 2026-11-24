<?php

function run(int $ttl, $mixed, string $path)
{
    setcookie('a', 'b', $ttl);
    setcookie('a', 'b', $mixed);
    setcookie('a', 'b', $mixed, $path);
    setcookie('a', 'b', time() + 60, '/', 'example.com', true, true);
    setcookie('a', 'b', [ 'expires' => 1 ]);
    setcookie('a', 'b', array('expires' => 1), '/');
    setrawcookie('a', 'b', 0, '/');
    \setcookie('a', 'b', 0, '/');
}
-----
<?php

function run(int $ttl, $mixed, string $path)
{
    setcookie('a', 'b', ['expires' => $ttl]);
    setcookie('a', 'b', $mixed);
    setcookie('a', 'b', ['expires' => $mixed, 'path' => $path]);
    setcookie('a', 'b', ['expires' => time() + 60, 'path' => '/', 'domain' => 'example.com', 'secure' => true, 'httponly' => true]);
    setcookie('a', 'b', [ 'expires' => 1 ]);
    setcookie('a', 'b', array('expires' => 1), '/');
    setrawcookie('a', 'b', ['expires' => 0, 'path' => '/']);
    \setcookie('a', 'b', ['expires' => 0, 'path' => '/']);
}
