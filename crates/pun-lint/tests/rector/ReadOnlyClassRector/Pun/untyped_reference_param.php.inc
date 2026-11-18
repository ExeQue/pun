<?php

// Rector 2.8.4 reprints an untyped parameter's modifiers up to its variable and drops the `&` between them
// (`protected readonly &$b` becomes `protected $b`), so the property is no longer set by reference. Pun keeps it.

namespace App;

readonly class ReadonlyBase
{
}

class Open extends ReadonlyBase
{
    public function __construct(protected readonly &$b)
    {
    }
}
-----
<?php

// Rector 2.8.4 reprints an untyped parameter's modifiers up to its variable and drops the `&` between them
// (`protected readonly &$b` becomes `protected $b`), so the property is no longer set by reference. Pun keeps it.

namespace App;

readonly class ReadonlyBase
{
}

readonly class Open extends ReadonlyBase
{
    public function __construct(protected &$b)
    {
    }
}
