<?php

namespace App;

function check($a)
{
    if ($a) {
        return \true;
    }

    return is_int($a);
}

final class Checks
{
    public function viaMethod($a)
    {
        if ($a === null) {
            return false;
        }

        return $this->isValid($a);
    }

    public function viaStatic($a)
    {
        if ($a === null) {
            return FALSE;
        }

        return self::valid($a);
    }

    public function maybeVoid($a)
    {
        if ($a) {
            return true;
        }
        if ($a === 1) {
            return $a > 1;
        }
    }

    public function withClosure($a)
    {
        $f = function () {
            return 1;
        };

        if ($a) {
            return (true);
        }

        return ! $a;
    }

    public function untypedCall($a)
    {
        if ($a) {
            return true;
        }

        return $this->unknown($a);
    }

    private function isValid($a): bool
    {
        return true;
    }

    private static function valid($a): bool
    {
        return true;
    }

    private function unknown($a)
    {
        return $a;
    }
}
-----
<?php

namespace App;

function check($a): bool
{
    if ($a) {
        return \true;
    }

    return is_int($a);
}

final class Checks
{
    public function viaMethod($a): bool
    {
        if ($a === null) {
            return false;
        }

        return $this->isValid($a);
    }

    public function viaStatic($a): bool
    {
        if ($a === null) {
            return FALSE;
        }

        return self::valid($a);
    }

    public function maybeVoid($a)
    {
        if ($a) {
            return true;
        }
        if ($a === 1) {
            return $a > 1;
        }
    }

    public function withClosure($a): bool
    {
        $f = function () {
            return 1;
        };

        if ($a) {
            return (true);
        }

        return ! $a;
    }

    public function untypedCall($a)
    {
        if ($a) {
            return true;
        }

        return $this->unknown($a);
    }

    private function isValid($a): bool
    {
        return true;
    }

    private static function valid($a): bool
    {
        return true;
    }

    private function unknown($a)
    {
        return $a;
    }
}
