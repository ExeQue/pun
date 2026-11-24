<?php

class Shapes
{
    private $items = [];

    public function first($key)
    {
        if (isset($this->items[$key])) {
            return $this->items[$key];
        }
        // fallback
        return $this->items[0] ?? null;
    }

    public function second($key)
    {
        if (isset($this->items[$key], $this->items[0])) {
            return $this->items[$key];
        }
        return $this->items[0] = 'x';
    }

    public function third($key)
    {
        if (isset($this->items[$key])) {
            return $this->items[$key];
        } else {
            $x = 1;
        }
        return 'x';
    }

    public function fourth($key)
    {
        if (isset($this->items[$key])) {
            return $this->items[0];
        }
        return 'x';
    }

    public function fifth($key)
    {
        if (isset($this->items[$key])) return $this->items[$key];
        return $key and $this;
    }

    public function sixth($key)
    {
        if (!isset($this->items[$key])) {
            return $this->items[$key];
        }
        return 'x';
    }

    public function seventh($key)
    {
        if (isset($key)) {
            return $key;
        }
        return;
    }
}
-----
<?php

class Shapes
{
    private $items = [];

    public function first($key)
    {
        // fallback
        return $this->items[$key] ?? $this->items[0] ?? null;
    }

    public function second($key)
    {
        return $this->items[$key] ?? $this->items[0] = 'x';
    }

    public function third($key)
    {
        if (isset($this->items[$key])) {
            return $this->items[$key];
        } else {
            $x = 1;
        }
        return 'x';
    }

    public function fourth($key)
    {
        if (isset($this->items[$key])) {
            return $this->items[0];
        }
        return 'x';
    }

    public function fifth($key)
    {
        return $this->items[$key] ?? ($key and $this);
    }

    public function sixth($key)
    {
        if (!isset($this->items[$key])) {
            return $this->items[$key];
        }
        return 'x';
    }

    public function seventh($key)
    {
        if (isset($key)) {
            return $key;
        }
        return;
    }
}
