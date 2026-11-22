<?php

final class Callables
{
    public function run(): void
    {
        $this->named();
        $this->otherCase();
        $this->doubleQuoted();
        $this->interpolated();
        $this->heredoc();
        $this->arrayCallable();
        $this->firstClass();
        $this->staticFirstClass();

        $log = 'calling named';
        $name = 'OTHERCASE';
        $double = "doubleQuoted";
        $text = "interpolated {$name}";
        $doc = <<<TXT
        heredoc
        TXT;
        usort($items, [self::class, 'arrayCallable']);
        $first = $this->firstClass(...);
        $static = self::staticFirstClass(...);
    }

    private function named(): int { return 1; }
    private function otherCase(): int { return 1; }
    private function doubleQuoted(): int { return 1; }
    private function interpolated(): int { return 1; }
    private function heredoc(): int { return 1; }
    private function arrayCallable(): int { return 1; }
    private function firstClass(): int { return 1; }
    private static function staticFirstClass(): int { return 1; }
}
