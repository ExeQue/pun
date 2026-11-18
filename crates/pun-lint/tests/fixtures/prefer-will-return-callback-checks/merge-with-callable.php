<?php

namespace App;

use PHPUnit\Framework\TestCase;

final class CallbacksTest extends TestCase
{
    public function testMerge()
    {
        $this->createMock('SomeClass')
            ->method('someMethod')
            ->willReturnCallback(function ($arg): string {
                return 'someValue';
            });
    }

    public function testVoid(): void
    {
        $this->createMock(\SplObjectStorage::class)
            ->method('detach')
            ->with($this->callback(function ($entity) {
                $this->assertInstanceOf(\stdClass::class, $entity);
                return true;
            }));
    }
}
