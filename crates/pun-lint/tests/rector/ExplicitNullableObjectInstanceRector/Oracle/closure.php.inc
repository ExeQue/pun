<?php

namespace Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle;

use Closure;

final class ClosureProperty
{
    private ?Closure $callback = null;

    public function run(?Closure $other)
    {
        if ($this->callback) {
            return 1;
        }

        if (! $other) {
            return 2;
        }

        return 0;
    }
}
-----
<?php

namespace Rector\Tests\CodeQuality\Rector\If_\ExplicitNullableObjectInstanceRector\Oracle;

use Closure;

final class ClosureProperty
{
    private ?Closure $callback = null;

    public function run(?Closure $other)
    {
        if ($this->callback instanceof \Closure) {
            return 1;
        }

        if (!$other instanceof \Closure) {
            return 2;
        }

        return 0;
    }
}
