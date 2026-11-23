<?php

namespace App;

function go($foo)
{
    return is_array($foo) || $foo instanceof Countable;
}
-----
<?php

namespace App;

function go($foo)
{
    return is_array($foo) || $foo instanceof Countable;
}
