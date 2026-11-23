<?php

namespace App;

final class Heredoc
{
    public readonly string $c;

    public function __construct()
    {
        $this->c = <<<TXT
            hi
            TXT;
    }
}

$object = new class {
    public readonly int $a;

    public function __construct()
    {
        $this->a = 1;
    }
};
-----
<?php

namespace App;

final class Heredoc
{
    public readonly string $c = <<<TXT
            hi
            TXT;

    public function __construct()
    {
    }
}

$object = new class {
    public readonly int $a = 1;

    public function __construct()
    {
    }
};
