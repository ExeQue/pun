<?php

namespace Pun\Oracle\MyCLabsCall;

use MyCLabs\Enum\Enum;

/**
 * @method static Status ACTIVE()
 */
final class Status extends Enum
{
    const ACTIVE = 'active';

    public static function create(): self
    {
        return self::ACTIVE();
    }

    public function run(): void
    {
        $a = self::ACTIVE()->getKey();
        $b = static::ACTIVE();
    }
}

final class Usage
{
    public function run(Status $status, ?Status $maybe)
    {
        $a = Status::create()->getKey();
        $b = Status::create()->getValue();
        $c = !$status->equals(Status::ACTIVE());
        $d = $status->equals(foo(Status::ACTIVE()));
        $e = $status->equals();
        $f = (new Status('active'))->getValue();
        $g = $maybe->getKey();
        $h = Status::from('active')->getKey();
        $i = $status->equals(...[Status::ACTIVE()]);
    }
}
-----
<?php

namespace Pun\Oracle\MyCLabsCall;

use MyCLabs\Enum\Enum;

/**
 * @method static Status ACTIVE()
 */
final class Status extends Enum
{
    const ACTIVE = 'active';

    public static function create(): self
    {
        return self::ACTIVE();
    }

    public function run(): void
    {
        $a = self::ACTIVE->name;
        $b = static::ACTIVE();
    }
}

final class Usage
{
    public function run(Status $status, ?Status $maybe)
    {
        $a = \Pun\Oracle\MyCLabsCall\Status::create->name;
        $b = \Pun\Oracle\MyCLabsCall\Status::create->value;
        $c = !($status === \Pun\Oracle\MyCLabsCall\Status::ACTIVE);
        $d = $status->equals(foo(\Pun\Oracle\MyCLabsCall\Status::ACTIVE));
        $e = $status->equals();
        $f = (new Status('active'))->value;
        $g = $maybe->name;
        $h = Status::from('active')->getKey();
        $i = $status->equals(...[\Pun\Oracle\MyCLabsCall\Status::ACTIVE]);
    }
}
