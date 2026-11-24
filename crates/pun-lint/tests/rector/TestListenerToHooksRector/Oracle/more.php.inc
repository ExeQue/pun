<?php

namespace App\Tests;

use PHPUnit\Framework\Test;
use PHPUnit\Framework\TestListener;

final class CountingListener implements TestListener, \Countable
{
    public function startTest(Test $test): void
    {
        echo 'start';
    }

    public function endTest(Test $test, float $time): void
    {
    }

    public function count(): int
    {
        return 0;
    }
}

class BaseListener implements TestListener
{
}

final class ChildListener extends BaseListener
{
    public function endTestSuite($suite): void
    {
        echo 'end';
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\Test;
use PHPUnit\Framework\TestListener;

final class CountingListener implements \Countable, \PHPUnit\Runner\BeforeTestHook
{
    public function executeBeforeTest(Test $test): void
    {
        echo 'start';
    }

    public function count(): int
    {
        return 0;
    }
}

class BaseListener
{
}

final class ChildListener extends BaseListener implements \PHPUnit\Runner\AfterLastTestHook
{
    public function executeAfterLastTest($suite): void
    {
        echo 'end';
    }
}
