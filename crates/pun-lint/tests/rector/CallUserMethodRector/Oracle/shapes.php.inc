<?php

function call($obj, $args)
{
    call_user_method('run', $obj);
    \CALL_USER_METHOD_ARRAY('run', $obj->inner(), [1, 2]);
    call_user_method('run', $obj, 1, 2);
}
-----
<?php

function call($obj, $args)
{
    call_user_func([$obj, 'run']);
    \CALL_USER_METHOD_ARRAY('run', $obj->inner(), [1, 2]);
    call_user_func([$obj, 'run'], 1, 2);
}
