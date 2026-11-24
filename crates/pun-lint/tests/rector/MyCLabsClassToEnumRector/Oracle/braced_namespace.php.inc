<?php

namespace App {
    use MyCLabs\Enum\Enum;

    /** Its doc */
    final class Kind extends Enum
    {
        const A = 'a' . 'b';
        const B = 'c';

        public static function first(): self
        {
            return self::A();
        }
    }
}
-----
<?php

namespace App {
    use MyCLabs\Enum\Enum;

    enum Kind : string
    {
        case A = 'a' . 'b';
        case B = 'c';
        public static function first(): self
        {
            return self::A();
        }
    }
}
