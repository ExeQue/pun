<?php

class Shapes
{
    private ?array $nullable = null;
    private array $typed = [];
    /** @var array|null */
    private $documented;
    private static ?self $instance = null;
    private static int $count = 0;

    public function run($a, $b)
    {
        if (!isset($a['x'])):
            $a['x'] = 1;
        endif;

        if (null === $b) {
            // inside
            $b = 2;
        }

        if ((is_null($a))) {
            $a = 3;
        }

        if (!isset($this->typed)) {
            $this->typed = [];
        }

        if ($this->nullable === null) {
            $this->nullable = [];
        }

        if ($this->documented === null) {
            $this->documented = [];
        }

        if (self::$instance === null) {
            self::$instance = new self();
        }

        if (static::$count === null) {
            static::$count = 1;
        }

        if ($a !== null) {
            $a = 4;
        }

        if ($a == null) {
            $a = 5;
        }

        if ($a === null) {
            $a .= 'x';
        }

        if ($a === null) {
            $a = [$a];
        }

        if (!isset($a, $b)) {
            $a = 6;
        }

        if ($a === null) $a = 7;

        if ($a)
            if ($b === null) {
                $b = 8;
            }
    }
}
-----
<?php

class Shapes
{
    private ?array $nullable = null;
    private array $typed = [];
    /** @var array|null */
    private $documented;
    private static ?self $instance = null;
    private static int $count = 0;

    public function run($a, $b)
    {
        $a['x'] ??= 1;

        // inside
        $b ??= 2;

        $a ??= 3;

        if (!isset($this->typed)) {
            $this->typed = [];
        }

        $this->nullable ??= [];

        $this->documented ??= [];

        self::$instance ??= new self();

        if (static::$count === null) {
            static::$count = 1;
        }

        if ($a !== null) {
            $a = 4;
        }

        if ($a == null) {
            $a = 5;
        }

        if ($a === null) {
            $a .= 'x';
        }

        if ($a === null) {
            $a = [$a];
        }

        if (!isset($a, $b)) {
            $a = 6;
        }

        $a ??= 7;

        if ($a)
            $b ??= 8;
    }
}
