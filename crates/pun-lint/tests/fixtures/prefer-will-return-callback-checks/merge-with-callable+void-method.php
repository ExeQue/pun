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
            ->willReturnCallback(function ($entity): void {
                $this->assertInstanceOf(\stdClass::class, $entity);
            });
    }
}
