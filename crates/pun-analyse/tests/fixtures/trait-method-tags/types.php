<?php

namespace TraitMethodTags;

use function PHPStan\dumpType;

interface Json
{
    public function toJson(): ?string;
}

trait Converter
{
    public function toJson(): ?string
    {
        return null;
    }

    public function toIso(): ?string
    {
        return null;
    }
}

/**
 * @method string toJson()
 */
trait Serialization
{
}

trait Date
{
    use Converter;
    use Serialization;
}

final class Immutable implements Json
{
    use Date;
}

final class Plain
{
    use Date;
}

final class Untagged implements Json
{
    use Converter;
}

function f(Immutable $immutable, Plain $plain, Untagged $untagged, Json $json): void
{
    dumpType($immutable->toJson());
    dumpType($plain->toJson());
    dumpType($plain->toIso());
    dumpType($untagged->toJson());
    dumpType($json->toJson());
}
