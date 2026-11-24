<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;
use PHPUnit\Framework\TestCase\Warning;

final class PhpErrorMoreTest extends TestCase
{
    public function test()
    {
        $this->expectException(Warning::class);
        self::expectException(\PHPUnit\Framework\TestCase\Notice::class);
        $this->expectException('PHPUnit\Framework\TestCase\Deprecated');
        $this->expectException(\RuntimeException::class);
        $this->expectException();
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;
use PHPUnit\Framework\TestCase\Warning;

final class PhpErrorMoreTest extends TestCase
{
    public function test()
    {
        $this->expectWarning();
        self::expectNotice();
        $this->expectException('PHPUnit\Framework\TestCase\Deprecated');
        $this->expectException(\RuntimeException::class);
        $this->expectException();
    }
}
