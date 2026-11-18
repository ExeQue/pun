<?php

test('expects an exception', function () {
    $this->expectExceptionObject($exception);
    doSomething();
});

test('it throws', function () {
    try {
        riskyOperation();
    } catch (\InvalidArgumentException $e) {
    }
});
