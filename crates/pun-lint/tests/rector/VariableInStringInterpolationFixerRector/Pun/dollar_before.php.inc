<?php

// Rector 2.8.4 writes "${$v}" for "$${v}": the literal `$` and the new `{$` make a variable-variable. Pun escapes
// that `$` so the string keeps its value (checked with PHP 8.5: each pair below is ===).

$v = 'x';
function foo() { return 'v'; }
echo "$${v}";
echo "\$${v}";
echo "\\$${v}";
echo "$${foo()}";

?>
-----
<?php

// Rector 2.8.4 writes "${$v}" for "$${v}": the literal `$` and the new `{$` make a variable-variable. Pun escapes
// that `$` so the string keeps its value (checked with PHP 8.5: each pair below is ===).

$v = 'x';
function foo() { return 'v'; }
echo "\${$v}";
echo "\${$v}";
echo "\\\${$v}";
echo "\${${foo()}}";

?>
