<?php

function run($name, $object)
{
    echo $$name;
    $$name->foo();
    $$name::bar();
    echo $$$name;
    echo ${$name};
    echo "$$name";
    $object->$name;
    return isset($$name['key']);
}
-----
<?php

function run($name, $object)
{
    echo ${$name};
    ${$name}->foo();
    ${$name}::bar();
    echo ${${$name}};
    echo ${$name};
    echo "$$name";
    $object->$name;
    return isset(${$name}['key']);
}
