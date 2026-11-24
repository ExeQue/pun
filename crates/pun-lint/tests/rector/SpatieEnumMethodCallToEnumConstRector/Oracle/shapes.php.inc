<?php

namespace App;

use Spatie\Enum\Enum;

/**
 * @method static self draft()
 * @method static self inReview()
 */
class Status extends Enum
{
    public static function first(): self
    {
        return self::draft();
    }

    public function same(): bool
    {
        return static::inReview()->label() === self::draft()->value();
    }
}

class Plain
{
    public static function draft(): self
    {
        return new self();
    }

    public function getName(): string
    {
        return '';
    }
}

$a = Status::draft()->getValue();
$b = (Status::inReview())->getName();
$c = Status::draft()->other();
$d = Status::from('draft');
$e = Status::toArray();
$f = Plain::draft()->getName();
$g = Status::{'draft'}();
$h = Status::draft()?->getName();
$i = Status::DRAFT();
-----
<?php

namespace App;

use Spatie\Enum\Enum;

/**
 * @method static self draft()
 * @method static self inReview()
 */
class Status extends Enum
{
    public static function first(): self
    {
        return self::DRAFT;
    }

    public function same(): bool
    {
        return static::INREVIEW->name === self::DRAFT->value;
    }
}

class Plain
{
    public static function draft(): self
    {
        return new self();
    }

    public function getName(): string
    {
        return '';
    }
}

$a = \App\Status::DRAFT->value;
$b = \App\Status::INREVIEW->name;
$c = \App\Status::DRAFT->other();
$d = Status::from('draft');
$e = Status::toArray();
$f = Plain::draft()->getName();
$g = Status::{'draft'}();
$h = \App\Status::DRAFT?->getName();
$i = \App\Status::DRAFT;
