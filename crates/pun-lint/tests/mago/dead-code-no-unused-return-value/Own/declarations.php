<?php

final class Declarations
{
    public function run(): void
    {
        $this->publicMethod();
        $this->protectedMethod();
        $this->__helper();
        $this->_single();
        $this->neverReturns();
        $this->voidReturn();
        $this->untyped();
        $this->nullable();
    }

    public function publicMethod(): int { return 1; }
    protected function protectedMethod(): int { return 1; }
    private function __helper(): int { return 1; }
    private function _single(): int { return 1; }
    private function neverReturns(): never { throw new Exception(); }
    private function voidReturn(): void {}
    private function untyped() { return 1; }
    private function nullable(): ?int { return null; }
}

abstract class WithParent extends Base implements Contract
{
    public function run(): void
    {
        $this->compute();
    }

    private function compute(): int { return 1; }

    abstract protected function other(): int;
}

interface Contract
{
    public function run(): void;
}

trait Helpers
{
    public function run(): void
    {
        $this->helper();
    }

    private function helper(): int { return 1; }
}
