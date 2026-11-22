<?php

final class Ids
{
    private ?array $ids = null;
    private static ?array $cache = null;

    public function get(): array
    {
        $local = $this->ids ?? ($this->ids = $this->read());
        $cached = self::$cache ?? self::$cache = $this->read();
        $item = $local['key'] ?? ($local['key'] = 1);

        return $this->load()->ids ?? ($this->load()->ids = []);
    }
}
