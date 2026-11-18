<?php

test('expects an exception', function () {
    expect(fn() => doSomething())->toThrow($exception);
});

test('it throws', function () {
    expect(fn() => riskyOperation())->toThrow(\InvalidArgumentException::class);
});
