<?php

namespace App;

final class Asym
{
    public protected(set) readonly int $p;

    public function __construct(public private(set) readonly int $q)
    {
    }
}
-----
<?php

namespace App;

final readonly class Asym
{
    public protected(set) int $p;

    public function __construct(public private(set) int $q)
    {
    }
}
