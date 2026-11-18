<?php

namespace PunMixinMacros;

use Illuminate\Database\Query\Builder;
use Illuminate\Support\Collection;
use Illuminate\Support\Str;

use function PHPStan\Testing\assertType;

interface HasNames
{
    public const LIST = ['punLambda'];
}

class Names
{
    public const LIST = ['punMu'];
}

abstract class AbstractMixin
{
    abstract public function punAbstract(): \Closure;
}

class OtherMixin
{
    public function punOther(): \Closure
    {
        return fn (): int => 9;
    }
}

class Macros
{
    private const NAMES = ['punGamma', 'punDelta'];

    public static function register(): void
    {
        Str::mixin(new PunStrMixin());
        Builder::mixin(new QueryMixin());
        foreach (['punAlpha', 'punBeta'] as $name) {
            Str::macro($name, fn (): string => $name);
        }
        foreach (self::NAMES as $name) {
            Collection::macro($name, fn (): int => 2);
        }
        foreach (['punUpper' => 'upper', 'punLower' => 'lower'] as $macro => $method) {
            Str::macro($macro, fn (string $s): string => Str::$method($s));
        }
        foreach (self::missing() as $name) {
            Str::macro($name, fn (): int => 1);
        }
        Str::mixin(new OtherMixin());
        $other = new OtherMixin();
        Collection::mixin($other);
        foreach (array('punEpsilon') as $name) {
            Str::macro($name, fn (): int => 5);
        }
        foreach ([0 => 'punZeta', ...self::NAMES] as $i => $name) {
            Str::macro($name, fn (): int => 6);
        }
        foreach (Names::LIST as $name) {
            Str::macro($name, fn (): int => 7);
        }
        $names = new Names();
        foreach ($names::LIST as $name) {
            Collection::macro($name, fn (): int => 7);
        }
        foreach (Names::{'LIST'} as $name) {
            Builder::macro($name, fn (): int => 7);
        }
        foreach (HasNames::LIST as $name) {
            Str::macro($name, fn (): int => 8);
        }
        foreach ([['punIota']] as [$name]) {
            Str::macro($name, fn (): int => 10);
        }
        Str::macro('pun'.'Kappa', fn (): int => 11);
        Str::macro('punNope', fn (): Nope => throw new \RuntimeException());
    }

    /** @return list<string> */
    public static function missing(): array
    {
        return [];
    }
}

class QueryMixin
{
    public function punCount(): \Closure
    {
        return function (): int {
            return 3;
        };
    }
}

function test(Builder $query): void
{
    assertType('string', Str::punShout('a'));
    assertType('int', Str::punWhisper('a'));
    assertType('bool', Str::punHidden());
    assertType('int', $query->punCount());
    assertType('string', Str::punAlpha());
    assertType('string', Str::punBeta());
    assertType('string', Str::punUpper('a'));
    assertType('string', Str::punLower('a'));
    assertType('int', collect([1])->punGamma());
    assertType('int', (new Collection())->punDelta());
    assertType('int', Str::punOther());
    assertType('int', Str::punEpsilon());
    assertType('int', Str::punZeta());
    assertType('int', Str::punMu());
}
