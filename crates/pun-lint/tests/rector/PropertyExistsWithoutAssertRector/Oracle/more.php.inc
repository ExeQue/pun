<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PropertyExistsMoreTest extends TestCase
{
    public function test(object $object)
    {
        $this->assertClassNotHasAttribute('property', \stdClass::class, 'message');
        $this->objectHasAttribute('property', $object);
        $this->classHasAttribute('property', 'stdClass');
        self::assertClassHasAttribute('property', 'stdClass');
        $this->assertClassHasAttribute('property');
    }
}
-----
<?php

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class PropertyExistsMoreTest extends TestCase
{
    public function test(object $object)
    {
        $this->assertFalse(property_exists(\stdClass::class, 'property'), 'message');
        $this->assertTrue(property_exists($object, 'property'));
        $this->assertTrue(property_exists('stdClass', 'property'));
        self::assertClassHasAttribute('property', 'stdClass');
        $this->assertClassHasAttribute('property');
    }
}
