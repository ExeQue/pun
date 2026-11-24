<?php

namespace App;

use Lib\Gone;

$x = new class implements Gone {
};
-----
<?php

namespace App;

use Lib\Gone;

$x = new class {
};
