<?php

rand(5, 1);
rand(-1, -5);
rand(1, 5);
\RAND();
getrandmax();
rand(...);
rand(0x10, 2);
-----
<?php

random_int(1, 5);
random_int(-5, -1);
random_int(1, 5);
random_int(0, mt_getrandmax());
mt_getrandmax();
rand(...);
random_int(2, 0x10);
