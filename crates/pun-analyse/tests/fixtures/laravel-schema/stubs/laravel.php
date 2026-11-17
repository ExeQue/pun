<?php

// The Laravel and Carbon classes the fixture names, for Pun's test (the oracle run has the real vendor).

namespace Illuminate\Database\Eloquent {
    abstract class Model
    {
        public function __get($key)
        {
            return null;
        }
    }
}

namespace Illuminate\Database\Migrations {
    abstract class Migration
    {
    }
}

namespace Illuminate\Database\Schema {
    class Blueprint
    {
    }
}

namespace Illuminate\Support\Facades {
    class Facade
    {
    }

    class Schema extends Facade
    {
    }
}

namespace Illuminate\Support\Traits {
    trait Macroable
    {
        public static function macro($name, $macro)
        {
        }

        public static function __callStatic($method, $parameters)
        {
        }

        public function __call($method, $parameters)
        {
        }
    }
}

namespace Illuminate\Support {
    class Str
    {
        use \Illuminate\Support\Traits\Macroable;
    }
}

namespace Carbon {
    class Carbon
    {
    }
}
