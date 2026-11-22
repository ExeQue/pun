<?php

final class Members
{
    public int $hooked {
        get {
            $this->fromHook();

            return 1;
        }
    }

    public int $short {
        get => $this->fromShortHook();
    }

    public const VALUE = 'fromConstant';

    public function run(): void
    {
        $this->fromConstant();
        $this->fromShortHook();
    }

    private function fromHook(): int { return 1; }
    private function fromShortHook(): int { return 1; }
    private function fromConstant(): int { return 1; }
}

enum Suit: string
{
    case Hearts = 'H';

    public static function run(): void
    {
        Suit::fromEnum();
        self::fromSelf();
    }

    private static function fromEnum(): self { return self::Hearts; }
    private static function fromSelf(): ?self { return null; }
}

function outside(): void
{
    $object = new class {
        public function run(): void
        {
            $this->advance();
            $this->advance();
        }

        private function advance(): int { return 1; }
    };
}
