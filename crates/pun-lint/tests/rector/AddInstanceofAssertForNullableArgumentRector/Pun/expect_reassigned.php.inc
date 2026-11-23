<?php

declare(strict_types=1);

namespace Rector\PHPUnit\Tests\CodeQuality\Rector\ClassMethod\AddInstanceofAssertForNullableArgumentRector\Pun;

use PHPUnit\Framework\TestCase;
use Rector\PHPUnit\Tests\CodeQuality\Rector\ClassMethod\AddInstanceofAssertForNullableArgumentRector\Source\SomeClassUsedInTests;
use Rector\PHPUnit\Tests\CodeQuality\Rector\ClassMethod\AddInstanceofAssertForNullableArgumentRector\Source\SomeFactory;

final class ExpectReassigned extends TestCase
{
    public function test(SomeFactory $someFactory): void
    {
        $someObject = $someFactory->create();
        expect($someObject)->toBeInstanceOf(SomeClassUsedInTests::class);
        $someFactory->process($someObject);
        $someObject = $someFactory->create();
        expect($someObject)->toBeInstanceOf(SomeClassUsedInTests::class);
        $someFactory->process($someObject);
    }
}
