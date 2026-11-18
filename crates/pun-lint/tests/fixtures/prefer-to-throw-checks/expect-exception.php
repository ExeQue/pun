<?php

test('expects an exception', function () {
    expect(fn() => doSomething())->toThrow($exception);
});

test('it throws', function () {
    try {
        riskyOperation();
    } catch (\InvalidArgumentException $e) {
    }
});
