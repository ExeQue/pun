<?php

enum Status
{
    case Active;

    public function touch(): void
    {
        $this->log();
    }

    private function log(): string
    {
        return 'logged';
    }
}

$object = new class {
    public function run(): void
    {
        $this?->prepare();
    }

    private function prepare(): array
    {
        return [];
    }
};
