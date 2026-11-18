<?php

if (json_decode(associative: true) !== null && json_last_error() === JSON_ERROR_NONE) {
    echo 1;
}
-----
<?php

if (json_validate()) {
    echo 1;
}
