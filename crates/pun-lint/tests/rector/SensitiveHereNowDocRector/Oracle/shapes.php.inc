<?php

function run()
{
    if (true) {
        $a = <<<'EOT'
        EOT_1 and EOT
          indented

        last
        EOT;
    }
    $b = <<<EOT
    nothing here
    EOT;
    $c = <<<EOT
    EOT_X {$b}
    EOT;
    $d = <<<"LBL"
    LBL_
    LBL;
}
-----
<?php

function run()
{
    if (true) {
        $a = <<<'EOT_WRAP'
        EOT_1 and EOT
          indented
        
        last
        EOT_WRAP;
    }
    $b = <<<EOT
    nothing here
    EOT;
    $c = <<<EOT
    EOT_X {$b}
    EOT;
    $d = <<<LBL_WRAP
    LBL_
    LBL_WRAP;
}
