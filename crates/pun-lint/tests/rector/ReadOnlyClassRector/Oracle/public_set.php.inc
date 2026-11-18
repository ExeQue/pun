<?php

namespace App;

final class PublicSet
{
    public public(set) readonly int $p;
}
-----
<?php

namespace App;

final readonly class PublicSet
{
    public public(set) int $p;
}
