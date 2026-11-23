<?php

namespace App;

define(FOO, 1);
\define(\BAR, 2);
define((BAZ), 3);
define(Qux\NAME, 4);
define('OK', 5);
define($name, 6);
define(...$args);
-----
<?php

namespace App;

define('FOO', 1);
\define('BAR', 2);
define(('BAZ'), 3);
define('App\Qux\NAME', 4);
define('OK', 5);
define($name, 6);
define(...$args);
