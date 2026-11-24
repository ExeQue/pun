<?php

function run($a)
{
    assert('is_int($a) && $a > 0', 'message');
    assert('return 1');
    assert("$a === 1");
    assert('');
}
-----
<?php

function run($a)
{
    assert(is_int($a) && $a > 0, 'message');
    assert('return 1');
    assert("$a === 1");
    assert('');
}
