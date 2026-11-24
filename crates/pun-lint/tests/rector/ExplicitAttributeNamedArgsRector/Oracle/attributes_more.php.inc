<?php

namespace App\Attrs;

#[\Attribute]
class Route
{
    public function __construct(string $path, array $methods = [], ?string $name = null) {}
}

#[\Attribute]
class Tags
{
    public function __construct(string ...$tags) {}
}

#[\Attribute]
class NoConstructor
{
}

#[Route('/a', ['GET'], 'a')]
#[Route('/b', name: 'b')]
#[Tags('x', 'y')]
#[NoConstructor]
#[Unknown('x')]
#[Route('/c', ['GET'], 'c', 'extra')]
class Controller
{
    #[Route('/d')]
    public function d(#[\SensitiveParameter] $secret) {}

    #[\Deprecated('use e2', '1.0')]
    public function e() {}
}

#[Route(
    '/f',
    ['POST'],
)]
function f() {}
-----
<?php

namespace App\Attrs;

#[\Attribute]
class Route
{
    public function __construct(string $path, array $methods = [], ?string $name = null) {}
}

#[\Attribute]
class Tags
{
    public function __construct(string ...$tags) {}
}

#[\Attribute]
class NoConstructor
{
}

#[Route(path: '/a', methods: ['GET'], name: 'a')]
#[Route(path: '/b', name: 'b')]
#[Tags('x', 'y')]
#[NoConstructor]
#[Unknown('x')]
#[Route('/c', ['GET'], 'c', 'extra')]
class Controller
{
    #[Route(path: '/d')]
    public function d(#[\SensitiveParameter] $secret) {}

    #[\Deprecated(message: 'use e2', since: '1.0')]
    public function e() {}
}

#[Route(
    path: '/f',
    methods: ['POST'],
)]
function f() {}
