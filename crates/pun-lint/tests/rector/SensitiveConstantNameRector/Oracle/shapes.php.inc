<?php

const lower_defined = 1;

function run()
{
    echo foo;
    echo \bar;
    echo php_eol;
    echo e_all;
    echo lower_defined;
    echo Mixed_Case;
    echo M_PI;
    echo m_pi;
}
-----
<?php

const lower_defined = 1;

function run()
{
    echo \FOO;
    echo \BAR;
    echo php_eol;
    echo e_all;
    echo lower_defined;
    echo \MIXED_CASE;
    echo M_PI;
    echo \M_PI;
}
