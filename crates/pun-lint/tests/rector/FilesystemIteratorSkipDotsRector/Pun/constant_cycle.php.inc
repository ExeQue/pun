<?php

// Rector 2.8.4 never finishes on this file (its child process times out after 120 seconds): resolving `Loop::A`
// follows `Loop::B` back to `Loop::A`. Pun stops following, so the value is unknown, and an unknown flag gets
// SKIP_DOTS as `Unknown::FLAG` does.

namespace App;

class Loop
{
    public const A = Loop::B;
    public const B = Loop::A;
}

new \FilesystemIterator(__DIR__, Loop::A);
-----
<?php

// Rector 2.8.4 never finishes on this file (its child process times out after 120 seconds): resolving `Loop::A`
// follows `Loop::B` back to `Loop::A`. Pun stops following, so the value is unknown, and an unknown flag gets
// SKIP_DOTS as `Unknown::FLAG` does.

namespace App;

class Loop
{
    public const A = Loop::B;
    public const B = Loop::A;
}

new \FilesystemIterator(__DIR__, Loop::A | \FilesystemIterator::SKIP_DOTS);
