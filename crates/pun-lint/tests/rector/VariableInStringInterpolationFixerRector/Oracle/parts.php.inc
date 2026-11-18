<?php
$v = 1; $a = 'v'; $o = new stdClass; $arr = ['x' => 1, 3 => 2];
echo "${$a}";
echo "${arr[3]}";
echo "${v}${v}";
echo <<<EOT
x ${v} y ${arr['x']}
EOT;
echo `ls ${v}`;
echo "${ v }";
echo "\\${v}";
-----
<?php
$v = 1; $a = 'v'; $o = new stdClass; $arr = ['x' => 1, 3 => 2];
echo "{${$a}}";
echo "{$arr[3]}";
echo "{$v}{$v}";
echo <<<EOT
x {$v} y {$arr['x']}
EOT;
echo `ls ${v}`;
echo "{${v}}";
echo "\\{$v}";
