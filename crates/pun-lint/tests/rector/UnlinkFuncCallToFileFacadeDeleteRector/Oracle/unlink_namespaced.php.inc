<?php

namespace App;

function run(string $path, $ctx)
{
    $a = @unlink($path);
    $b = unlink($path, $ctx);
    $c = \unlink(...);
    $d = unLink($path);
    return @(unlink($path));
}
-----
<?php

namespace App;

function run(string $path, $ctx)
{
    $a = \Illuminate\Support\Facades\File::delete($path);
    $b = unlink($path, $ctx);
    $c = \Illuminate\Support\Facades\File::delete(...);
    $d = \Illuminate\Support\Facades\File::delete($path);
    return \Illuminate\Support\Facades\File::delete($path);
}
