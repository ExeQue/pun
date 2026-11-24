<?php

class LegacyTest extends \PHPUnit_Framework_TestCase
{
    public function testWithoutDoc()
    {
        $nothing = 5;
    }

    /**
     * @test
     */
    public function documented()
    {
        $nothing = 5;
    }

    /** @group slow */
    public function testOneLineDoc()
    {
        $this->addToAssertionCount(1); // counted
    }
}
-----
<?php

class LegacyTest extends \PHPUnit_Framework_TestCase
{
    /**
     * @doesNotPerformAssertions
     */
    public function testWithoutDoc()
    {
        $nothing = 5;
    }

    /**
     * @test
     * @doesNotPerformAssertions
     */
    public function documented()
    {
        $nothing = 5;
    }

    /**
     * @group slow
     * @doesNotPerformAssertions
     */
    public function testOneLineDoc()
    {
    }
}
