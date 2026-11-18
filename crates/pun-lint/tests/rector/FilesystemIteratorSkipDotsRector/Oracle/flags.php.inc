<?php

namespace App;

use FilesystemIterator as FI;

final class Flags
{
    public const SKIP = \FilesystemIterator::SKIP_DOTS;
    public const OTHER = 1;
}

class MyIterator extends \FilesystemIterator
{
}

new FI(__DIR__, FI::KEY_AS_FILENAME);
new \RecursiveDirectoryIterator(__DIR__, \RecursiveDirectoryIterator::KEY_AS_FILENAME);
new \GlobIterator('*', \FilesystemIterator::KEY_AS_FILENAME);
new MyIterator(__DIR__, MyIterator::KEY_AS_FILENAME);
new \FilesystemIterator(__DIR__, Flags::SKIP);
new \FilesystemIterator(__DIR__, Flags::OTHER);
new \FilesystemIterator(__DIR__, Unknown::FLAG);
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME | (FI::UNIX_PATHS | FI::SKIP_DOTS));
new \FilesystemIterator(__DIR__, (FI::KEY_AS_FILENAME | FI::UNIX_PATHS));
new \FilesystemIterator(__DIR__, 256);
new \FilesystemIterator(__DIR__, flags: FI::KEY_AS_FILENAME);
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME, );
new \FilesystemIterator(
    __DIR__,
    FI::KEY_AS_FILENAME
);
new \DirectoryIterator(__DIR__);
-----
<?php

namespace App;

use FilesystemIterator as FI;

final class Flags
{
    public const SKIP = \FilesystemIterator::SKIP_DOTS;
    public const OTHER = 1;
}

class MyIterator extends \FilesystemIterator
{
}

new FI(__DIR__, FI::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
new \RecursiveDirectoryIterator(__DIR__, \RecursiveDirectoryIterator::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
new \GlobIterator('*', \FilesystemIterator::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
new MyIterator(__DIR__, MyIterator::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
new \FilesystemIterator(__DIR__, Flags::SKIP);
new \FilesystemIterator(__DIR__, Flags::OTHER | \FilesystemIterator::SKIP_DOTS);
new \FilesystemIterator(__DIR__, Unknown::FLAG | \FilesystemIterator::SKIP_DOTS);
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME | (FI::UNIX_PATHS | FI::SKIP_DOTS));
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME | FI::UNIX_PATHS | \FilesystemIterator::SKIP_DOTS);
new \FilesystemIterator(__DIR__, 256);
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS);
new \FilesystemIterator(__DIR__, FI::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS, );
new \FilesystemIterator(
    __DIR__,
    FI::KEY_AS_FILENAME | \FilesystemIterator::SKIP_DOTS
);
new \DirectoryIterator(__DIR__);
