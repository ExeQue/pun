<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ScenarioMoreTest extends TestCase
{
    /**
     * Does something.
     *
     * @scenario
     * @group slow
     */
    public function one()
    {
    }

    /** @scenario */
    public function two()
    {
    }

    /**
     * @scenario
     * @scenario
     */
    public function three()
    {
    }

    /**
     * @scenarios
     */
    public function four()
    {
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class ScenarioMoreTest extends TestCase
{
    /**
     * Does something.
     *
     * @group slow
     * @test
     */
    public function one()
    {
    }

    /** @test */
    public function two()
    {
    }

    /**
     * @test
     * @test
     */
    public function three()
    {
    }

    /**
     * @scenarios
     */
    public function four()
    {
    }
}
