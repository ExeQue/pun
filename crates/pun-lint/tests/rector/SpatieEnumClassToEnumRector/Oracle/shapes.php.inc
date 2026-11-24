<?php

namespace App;

use Spatie\Enum\Enum;

/**
 * Some text.
 *
 * @method static self draft()
 * @method static self inReview()
 * @method static Status published(string $why)
 */
final class Status extends Enum
{
    protected static function values(): array
    {
        if (rand()) {
            return ['draft' => 'd', 'inReview' => 2];
        }

        return array('draft' => "it's \\ here");
    }

    protected static function labels(): array
    {
        return [];
    }
}

/**
 * @method static self a()
 */
class NoValues extends Enum
{
}

class NoTags extends Enum
{
}

/**
 * @method static self a()
 * @method static self b()
 */
class Ints extends Enum
{
    protected static function values(): \Closure|array
    {
        return ['a' => 1, 'b' => 0x10, 'c' => 'x' . 'y'];
    }
}
-----
<?php

namespace App;

use Spatie\Enum\Enum;

enum Status : string
{
    case DRAFT = 'it\'s \ here';
    case INREVIEW = 2;
    case PUBLISHED = 'published';
}

enum NoValues : string
{
    case A = 'a';
}

enum NoTags
{
}

enum Ints : int
{
    case A = 1;
    case B = 16;
}
