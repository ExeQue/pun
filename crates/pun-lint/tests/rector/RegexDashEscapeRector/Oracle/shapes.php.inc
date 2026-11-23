<?php

function patterns()
{
    preg_match('#[a-z-\d]#', 'x');
    preg_match('#[\d-a][\s-b]#', 'x');
    preg_match("#[\\w-a]#", 'x');
    $p = ['#[A-\W]#', '#[\D-x]#i'];
    preg_match('#[x-\w]#', 'x');
    echo 'no regex [\s- here';
}
-----
<?php

function patterns()
{
    preg_match('#[a-z\-\d]#', 'x');
    preg_match('#[\d\-a][\s\-b]#', 'x');
    preg_match("#[\\w\-a]#", 'x');
    $p = ['#[A\-\W]#', '#[\D\-x]#i'];
    preg_match('#[x\-\w]#', 'x');
    echo 'no regex [\s\- here';
}
