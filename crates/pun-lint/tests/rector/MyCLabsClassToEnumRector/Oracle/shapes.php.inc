<?php

namespace App;

use MyCLabs\Enum\Enum;

abstract class Base extends Enum
{
}

// A line comment
/**
 * The status.
 */
#[\Attribute]
final class Status extends Base implements \JsonSerializable
{
    public const ONE = 1, TWO = 2;

    // three
    protected const THREE = 3;

    private $cache = [];

    /**
     * Its label.
     */
    public function label(): string
    {
        return   'x' ; // kept as written
    }

    public function jsonSerialize(): mixed
    {
        return 1;
    }
}

class Plain
{
    private const A = 'a';
}

final class Empty_ extends Enum
{
}
-----
<?php

namespace App;

use MyCLabs\Enum\Enum;

enum Base
{
}

enum Status : int
{
    case ONE = 1;
    // three
    case THREE = 3;
    /**
     * Its label.
     */
    public function label(): string
    {
        return   'x' ; // kept as written
    }
    public function jsonSerialize(): mixed
    {
        return 1;
    }
}

class Plain
{
    private const A = 'a';
}

enum Empty_
{
}
