<?php

final class Ignored
{
    public function run(): void
    {
        $this->nothing();
        $this->untyped();
        $this->open();
    }

    private function nothing(): void {}
    private function untyped() { return 1; }
    public function open(): int { return 1; }
    private function never(): int { return 1; }
}

final class UsesTrait
{
    use SomeTrait;

    public function run(): void
    {
        $this->advance();
    }

    private function advance(): int { return 1; }
}

final class CallsDynamically
{
    public function run(string $method): void
    {
        $this->advance();
        $this->{$method}();
    }

    private function advance(): int { return 1; }
}
