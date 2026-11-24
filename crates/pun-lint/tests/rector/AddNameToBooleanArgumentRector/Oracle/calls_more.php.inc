<?php

namespace App\Named;

function local($first, $second, $third = null) {}

/**
 * @no-named-arguments
 */
function forbidden($first, $second) {}

function rest($first, ...$rest) {}

class Box
{
    public function put($item, $strict = false, $default = null) {}

    public static function make($value, $strict = false) {}

    public function run(?Box $other, array $a, string $s)
    {
        $this->put($a, true);
        $other?->put($a, false, null);
        self::make(1, true);
        static::make(2, \TRUE);
        local(true, 1, 2);
        local(1, false, third: 3);
        forbidden(1, true);
        rest(true);
        rest(true, 1, 2);
        rest(1, true);
        unknown(true);
        $f = 'local';
        $f(1, true);
        in_array(true, $a);
        array_search($s, $a, true);
        htmlspecialchars($s, ENT_QUOTES, 'UTF-8', false);
        sprintf('%s', true);
        $x = new \ArrayObject([], 0, \ArrayIterator::class);
        str_pad($s, 10, ' ', STR_PAD_LEFT);
        array_filter($a, null, ARRAY_FILTER_USE_KEY);
        mb_substr($s, 0, null, 'UTF-8');
        json_decode($s, null, 512, JSON_THROW_ON_ERROR);
        $this->put(null, null, null);
        local(1, 2, NULL);
        $this->put(1,
            true
        );
        var_export($a, true);
        print_r($a, true);
        setcookie('a', 'b', 0, '/', '', true, true);
    }
}
-----
<?php

namespace App\Named;

function local($first, $second, $third = null) {}

/**
 * @no-named-arguments
 */
function forbidden($first, $second) {}

function rest($first, ...$rest) {}

class Box
{
    public function put($item, $strict = false, $default = null) {}

    public static function make($value, $strict = false) {}

    public function run(?Box $other, array $a, string $s)
    {
        $this->put($a, strict: true);
        $other?->put($a, false, null);
        self::make(1, strict: true);
        static::make(2, strict: \TRUE);
        local(first: true, second: 1, third: 2);
        local(1, second: false, third: 3);
        forbidden(1, true);
        rest(first: true);
        rest(true, 1, 2);
        rest(1, true);
        unknown(true);
        $f = 'local';
        $f(1, true);
        in_array(needle: true, haystack: $a);
        array_search($s, $a, strict: true);
        htmlspecialchars($s, ENT_QUOTES, 'UTF-8', double_encode: false);
        sprintf('%s', true);
        $x = new \ArrayObject([], 0, \ArrayIterator::class);
        str_pad($s, 10, ' ', STR_PAD_LEFT);
        array_filter($a, null, ARRAY_FILTER_USE_KEY);
        mb_substr($s, 0, null, 'UTF-8');
        json_decode($s, null, 512, JSON_THROW_ON_ERROR);
        $this->put(null, null, null);
        local(1, 2, NULL);
        $this->put(1,
            strict: true
        );
        var_export($a, return: true);
        print_r($a, return: true);
        setcookie('a', 'b', 0, '/', '', secure: true, httponly: true);
    }
}
