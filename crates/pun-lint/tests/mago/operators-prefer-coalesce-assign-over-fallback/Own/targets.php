<?php

final class Cache
{
    private static ?array $items = null;
    private ?array $map = null;

    public function get(string $key, string $class, string $name): mixed
    {
        $a = static::$items ?? (static::$items = []);
        $b = parent::$items ?? (parent::$items = []);
        $c = Cache::$items ?? (Cache::$items = []);
        $d = $class::$items ?? ($class::$items = []);
        $e = self::$$name ?? (self::$$name = []);
        $f = $this->map[$key] ?? ($this->map[$key] = 1);
        $g = $this->map['a'][2] ?? ($this->map['a'][2] = 1);
        $h = $this->map[KEY] ?? ($this->map[KEY] = 1);
        $i = $this->map[$key . 'x'] ?? ($this->map[$key . 'x'] = 1);
        $j = $this->{$name} ?? ($this->{$name} = 1);
        $k = $$name ?? ($$name = 1);
        $l = $this->map ?? ($this ->map = []);
        $m = $this->map ?? ($this->MAP = []);
        $n = $GLOBALS['x'] ?? ($GLOBALS['x'] = 1);
        $o = Cache::get()->map ?? (Cache::get()->map = 1);
        $p = $this->map[$this->key] ?? ($this->map[$this->key] = 1);

        return [$a, $b, $c, $d, $e, $f, $g, $h, $i, $j, $k, $l, $m, $n, $o, $p];
    }
}
