<?php

function run($HTTP_SESSION_VARS)
{
    return fn () => $HTTP_SESSION_VARS['user'];
}
