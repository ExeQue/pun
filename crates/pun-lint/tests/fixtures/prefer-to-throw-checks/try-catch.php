<?php

test('expects an exception', function () {
    $this->expectExceptionObject($exception);
    doSomething();
});

test('it throws', function () {
    expect(fn() => riskyOperation())->toThrow(\InvalidArgumentException::class);
});
