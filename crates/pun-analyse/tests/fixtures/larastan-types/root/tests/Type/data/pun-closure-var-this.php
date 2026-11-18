<?php

namespace PunClosureVarThis;

class Other
{
    public function known(): int
    {
        return 1;
    }
}

class Probe
{
    public function closures(): void
    {
        $f = function () {
            /** @var Other $this */
            $this->known();
            $this->unknown();
        };
        $g = function (): int {
            /** @var Other $this */
            return $this->known();
        };
        $h = function () {
            /** @var Probe $this */
            $this->closures();
            /** @var $this $this */
            $this->closures();
        };
        $k = function () {
            if (rand(0, 1) === 1) {
                /** @var Other $this */
                $this->known();
            }
            $this->closures();
        };
        $m = function () {
            /** @var Other $this */
            $x = $this->known();

            return $x;
        };
        $n = function () {
            /** @var Other $this */
            $this->known();
            /** @var Other $this */
            $this->known();
        };
        $o = new class {
            public function f(): void
            {
                /** @var Other $this */
                $this->known();
            }
        };
        /** @var Other $this */
        $this->known();
    }

    public static function inStatic(): void
    {
        $f = function () {
            return 1;
        };
    }
}
