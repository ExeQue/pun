<?php

namespace App;

use Illuminate\Console\Command;

class Child extends Command
{
    public function handle()
    {
        if (true) {
            $this->info('nested');
        }
        $this->INFO('upper');
        $this->comment('other');
        $x = $this->ask('assigned');
        $this->line('a')->line('b');
        $other->info('x');
        fn () => $this->info('closure');
    }
}

class GrandChild extends Child
{
    public function handle()
    {
        $this->warn('grand');
    }
}

class NotACommand
{
    public function handle()
    {
        $this->info('no');
    }
}
-----
<?php

namespace App;

use Illuminate\Console\Command;

class Child extends Command
{
    public function handle()
    {
        if (true) {
            $this->info('nested');
        }
        $this->components->INFO('upper');
        $this->comment('other');
        $x = $this->ask('assigned');
        $this->line('a')->line('b');
        $other->info('x');
        fn () => $this->info('closure');
    }
}

class GrandChild extends Child
{
    public function handle()
    {
        $this->components->warn('grand');
    }
}

class NotACommand
{
    public function handle()
    {
        $this->info('no');
    }
}
