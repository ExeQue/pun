<?php

namespace Lib {
    interface Base
    {
    }

    class Sub implements Base
    {
    }
}

namespace App {
    use Lib\Base;
    use Lib\Sub;

    interface InterfaceSkipped extends Base
    {
        public function make($a);
    }

    final class Maker implements Base
    {
        public function make(&$a, ...$rest): Sub
        {
        }

        public function build(?Base $a, int $b = 0): ?Base
        {
        }

        public function load(string $a)
        {
        }

        public function none()
        {
        }
    }

    final class NoParent
    {
        public function make($a)
        {
        }
    }
}
-----
<?php

namespace Lib {
    interface Base
    {
    }

    class Sub implements Base
    {
    }
}

namespace App {
    use Lib\Base;
    use Lib\Sub;

    interface InterfaceSkipped extends Base
    {
        public function make($a);
    }

    final class Maker implements Base
    {
        public function make(&$a, ...$rest): Sub
        {
        }

        public function build(?Base $a, int $b = 0): ?Base
        {
        }

        public function load(string $a): int|string
        {
        }

        public function none(): ?string
        {
        }
    }

    final class NoParent
    {
        public function make($a)
        {
        }
    }
}
