<?php

namespace App;

use Lib\Gone;

class A implements Gone
{
}

class B extends Base implements \Lib\Kept, Gone, \Lib\Gone2
{
}

class C implements Gone, \Lib\Kept
{
}

interface D extends \Lib\Kept, Gone
{
}
-----
<?php

namespace App;

use Lib\Gone;

class A
{
}

class B extends Base implements \Lib\Kept
{
}

class C implements \Lib\Kept
{
}

interface D extends \Lib\Kept
{
}
