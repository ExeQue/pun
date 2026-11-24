<?php

namespace App;

use Illuminate\Foundation\Testing\TestCase;

class SomeTest extends TestCase
{
    public function testSomething()
    {
        $this->withoutEvents()->get('/');
        $this->withoutMiddleware();
    }
}
-----
<?php

namespace App;

use Illuminate\Foundation\Testing\TestCase;

class SomeTest extends TestCase
{
    public function testSomething()
    {
        \Illuminate\Support\Facades\Event::fake()->get('/');
        $this->withoutMiddleware();
    }
}
