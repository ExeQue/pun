<?php

namespace Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle;

enum Status: string
{
    case Open = 'open';

    public static function parse(string $raw): ?self
    {
        return self::tryFrom($raw);
    }
}

final class EnumFactories
{
    public function tryFromResult(string $raw): bool
    {
        $status = Status::tryFrom($raw);

        return $status !== null;
    }

    public function tryFromCall(string $raw)
    {
        if (Status::tryFrom($raw)) {
            return 1;
        }

        return Status::tryFrom($raw) === null;
    }

    public function declaredNullableEnum(?Status $status, string $raw): bool
    {
        $parsed = Status::parse($raw);

        return $status !== null && $parsed !== null;
    }
}
-----
<?php

namespace Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle;

enum Status: string
{
    case Open = 'open';

    public static function parse(string $raw): ?self
    {
        return self::tryFrom($raw);
    }
}

final class EnumFactories
{
    public function tryFromResult(string $raw): bool
    {
        $status = Status::tryFrom($raw);

        return $status !== null;
    }

    public function tryFromCall(string $raw)
    {
        if (Status::tryFrom($raw)) {
            return 1;
        }

        return Status::tryFrom($raw) === null;
    }

    public function declaredNullableEnum(?Status $status, string $raw): bool
    {
        $parsed = Status::parse($raw);

        return $status instanceof \Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle\Status && $parsed instanceof \Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle\Status;
    }
}
