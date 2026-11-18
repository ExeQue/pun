<?php

namespace App;

$a = \json_decode($json) !== null && \json_last_error() === \JSON_ERROR_NONE;
$b = (json_decode($json, true) !== null) && (json_last_error() === JSON_ERROR_NONE);
$c = json_decode($json, false, 512, 1048576) !== null && json_last_error() === JSON_ERROR_NONE;
$d = $x || json_decode($json) !== null && json_last_error() === JSON_ERROR_NONE;
-----
<?php

namespace App;

$a = json_validate($json);
$b = json_validate($json);
$c = json_validate($json, 512, 1048576);
$d = $x || json_validate($json);
