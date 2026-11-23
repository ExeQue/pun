<?php

declare(strict_types=1);

namespace Rector\PHPUnit\Tests\CodeQuality\Rector\ClassMethod\AddInstanceofAssertForNullableInstanceRector\Pun;

use PHPUnit\Framework\TestCase;
use Rector\PHPUnit\Tests\CodeQuality\Rector\ClassMethod\AddInstanceofAssertForNullableInstanceRector\Source\SomeClassUsedInTests;

final class ExpectInstanceof extends TestCase
{
    public function test(): void
    {
        $someObject = $this->getSomeObject();
        expect($someObject)->toBeInstanceOf(SomeClassUsedInTests::class);
        $value = $someObject->getSomeMethod();

        $this->assertSame(123, $value);
    }

    public function testChained(): void
    {
        $someObject = $this->getSomeObject();
        expect($someObject)->toBeInstanceOf(SomeClassUsedInTests::class)
            ->and($someObject->getSomeMethod())->toBe(123);
        $value = $someObject->getSomeMethod();

        $this->assertSame(123, $value);
    }

    public function testAfterNot(): void
    {
        $someObject = $this->getSomeObject();
        expect($someObject)->not->toBeNull()
            ->toBeInstanceOf(SomeClassUsedInTests::class)
            ->and($someObject->getSomeMethod())->toBe(123);
    }

    public function testAndSubject(): void
    {
        $someObject = $this->getSomeObject();
        expect(123)->toBe(123)
            ->and($someObject)->toBeInstanceOf(SomeClassUsedInTests::class)
            ->and($someObject->getSomeMethod())->toBe(123);
    }

    private function getSomeObject(): ?SomeClassUsedInTests
    {
        if (mt_rand(0, 1)) {
            return new SomeClassUsedInTests();
        }

        return null;
    }
}
