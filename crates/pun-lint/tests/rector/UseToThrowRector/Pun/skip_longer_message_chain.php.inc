<?php

it('reports transport failures', function () {
    $client = new Client();

    try {
        $client->contact(42);
        $this->fail('Expected exception.');
    } catch (ApiException $exception) {
        expect($exception->getMessage())->toBe('Transport failed.')
            ->and($exception->getPrevious())->toBeNull()
            ->and($exception->context())->toBe(['reason' => 'transport']);
    }
});

it('keeps a negated message assertion', function () {
    try {
        foo();
    } catch (E $e) {
        expect($e->getMessage())->not->toBe('m');
    }
});

it('keeps separate assertions', function () {
    try {
        foo();
    } catch (E $e) {
        expect($e->getMessage())->toBe('m');
        expect($e->getPrevious())->toBeNull();
    }
});
