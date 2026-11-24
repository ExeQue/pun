<?php

function run($query)
{
    parse_str($query);
    echo 1;
    parse_str($query);
    $vars = array_merge(get_defined_vars(), ['a' => get_defined_vars()]);
    parse_str($query, $out);
    $x = get_defined_vars();
    parse_str($query);
}
-----
<?php

function run($query)
{
    parse_str($query, $result);
    echo 1;
    parse_str($query, $result);
    $vars = array_merge($result, ['a' => $result]);
    parse_str($query, $out);
    $x = get_defined_vars();
    parse_str($query);
}
