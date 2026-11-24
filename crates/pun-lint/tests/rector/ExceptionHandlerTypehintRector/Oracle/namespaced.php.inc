<?php

namespace App;

function handler(Exception $e)
{
}

function otherHandler(\Exception $e)
{
}
-----
<?php

namespace App;

function handler(Exception $e)
{
}

function otherHandler(\Throwable $e)
{
}
