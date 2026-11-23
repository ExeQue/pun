<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class AnyTest extends TestCase
{
    public function test(): void
    {
        $a = $this->any(1);
        $b = self::any();
        $c = $this->any(...);
        $d = $this->ANY();
        $e = $this->once();
        $this->other = $this->any();
        $f = $this->createMock(\stdClass::class)->any();
        $g = $this->{'any'}();
    }
}
-----
<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class AnyTest extends TestCase
{
    public function test(): void
    {
        $a = $this->any(1);
        $b = self::any();
        $c = $this->any(...);
        $d = new \PHPUnit\Framework\MockObject\Rule\AnyInvokedCount();
        $e = $this->once();
        $this->other = new \PHPUnit\Framework\MockObject\Rule\AnyInvokedCount();
        $f = $this->createMock(\stdClass::class)->any();
        $g = $this->{'any'}();
    }
}
