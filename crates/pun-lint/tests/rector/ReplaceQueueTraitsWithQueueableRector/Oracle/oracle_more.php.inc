<?php

namespace App;

use Illuminate\Bus\Queueable;
use Illuminate\Foundation\Bus\Dispatchable;
use Illuminate\Queue\InteractsWithQueue;
use Illuminate\Queue\SerializesModels;

class SomeJob
{
    use SomeOtherTrait, Dispatchable;
    use InteractsWithQueue, Queueable, SerializesModels;

    public function handle(): void
    {
    }
}
-----
<?php

namespace App;

use Illuminate\Bus\Queueable;
use Illuminate\Foundation\Bus\Dispatchable;
use Illuminate\Queue\InteractsWithQueue;
use Illuminate\Queue\SerializesModels;

class SomeJob
{
    use SomeOtherTrait, \Illuminate\Foundation\Queue\Queueable;

    public function handle(): void
    {
    }
}
