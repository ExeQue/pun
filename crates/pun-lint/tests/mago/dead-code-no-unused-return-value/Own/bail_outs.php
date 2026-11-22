<?php

final class MagicCall
{
    public function __call(string $name, array $arguments): mixed
    {
        return null;
    }

    public function run(): void
    {
        $this->advance();
    }

    private function advance(): int { return 1; }
}

final class VariableName
{
    public function run(string $method): void
    {
        $this->advance();
        $this->$method();
    }

    private function advance(): int { return 1; }
}

final class StaticDynamic
{
    public function run(string $method): void
    {
        $this->advance();
        self::{$method}();
    }

    private function advance(): int { return 1; }
}

final class DynamicCallable
{
    public function run(string $method): void
    {
        $this->advance();
        $callable = $this->$method(...);
    }

    private function advance(): int { return 1; }
}

final class NestedTrait
{
    public function run(): void
    {
        $this->advance();
        $object = new class {
            use SomeTrait;
        };
    }

    private function advance(): int { return 1; }
}

final class NestedDynamic
{
    public function run(): void
    {
        $this->advance();
        $object = new class {
            public function go(string $m): void
            {
                $this->$m();
            }
        };
    }

    private function advance(): int { return 1; }
}

#[Callback('advance')]
final class ClassAttribute
{
    public function run(): void
    {
        $this->advance();
    }

    private function advance(): int { return 1; }
}

final class MemberAttribute
{
    #[Callback('advance')]
    public function run(): void
    {
        $this->advance();
    }

    private function advance(): int { return 1; }
}

final class OuterCallInNested
{
    public function run(): void
    {
        $object = new class {
            public function go(): void
            {
                $this->advance();
            }

            private function advance(): string { return ''; }
        };
    }

    private function advance(): int { return 1; }
}
