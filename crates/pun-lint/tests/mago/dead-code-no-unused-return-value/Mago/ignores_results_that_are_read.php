<?php

final class Reader
{
    public function run(): int
    {
        $this->first();
        $value = $this->first();

        $this->second();
        echo $this->second();

        $this->third();
        $callable = $this->third(...);

        $this->fourth();
        array_map([$this, 'fourth'], []);

        $this->fifth();
        $values = [fn() => $this->fifth()];

        return $value;
    }

    private function first(): int { return 1; }
    private function second(): string { return ''; }
    private function third(): int { return 3; }
    private function fourth(): int { return 4; }
    private function fifth(): int { return 5; }
}
