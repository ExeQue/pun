<?php

namespace App;

use Illuminate\Support\Facades\Notification;

class SomeTest
{
    public function test()
    {
        Notification::assertTimesSent(3, \App\Notifications\Welcome::class);
        \Illuminate\Support\Facades\Notification::assertTimesSent(1, 'x');
        Notification::assertNothingSent();
    }
}
-----
<?php

namespace App;

use Illuminate\Support\Facades\Notification;

class SomeTest
{
    public function test()
    {
        Notification::assertSentTimes(\App\Notifications\Welcome::class, 3);
        \Illuminate\Support\Facades\Notification::assertSentTimes('x', 1);
        Notification::assertNothingSent();
    }
}
