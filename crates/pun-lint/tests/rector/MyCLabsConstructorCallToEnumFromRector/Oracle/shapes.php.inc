<?php

namespace App;

use MyCLabs\Enum\Enum;

final class Status extends Enum
{
    private const OPEN = 'open';

    public static function make(): self
    {
        return new static('open');
    }
}

class Child extends Status
{
}

class Plain
{
}

function run($value)
{
    $a = new Status($value);
    $b = new \App\Status('open');
    $c = new Child($value);
    $d = new Plain();
    $e = (new Status('open'))->getValue();
    $f = new Status;
}
-----
<?php

namespace App;

use MyCLabs\Enum\Enum;

final class Status extends Enum
{
    private const OPEN = 'open';

    public static function make(): self
    {
        return \App\Status::from('open');
    }
}

class Child extends Status
{
}

class Plain
{
}

function run($value)
{
    $a = \App\Status::from($value);
    $b = \App\Status::from('open');
    $c = \App\Child::from($value);
    $d = new Plain();
    $e = (\App\Status::from('open'))->getValue();
    $f = \App\Status::from();
}
