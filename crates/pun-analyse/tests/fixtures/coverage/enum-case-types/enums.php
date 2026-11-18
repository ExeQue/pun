<?php declare(strict_types=1);

namespace EnumCaseTypesCoverage;

enum Suit: string
{
    case Hearts = 'H';
    case Spades = 'S';

    const ALIAS = self::Hearts;
}

enum Num: int
{
    case One = 1;
    case Two = 2;
}

enum Plain
{
    case A;
    case B;
}

final class Holder
{
    public function __construct(public Suit $suit) {}
}

function f(string $str, int $int): void
{
    $a = Suit::from('H');
    $a::nope();
    $a->nope();
    echo $a->prop;
    $b = Suit::tryFrom('X');
    $b::nope();
    $c = Suit::tryFrom('S');
    $c->nope();
    $d = Num::from(2);
    $d::nope();
    $g = Suit::from($str);
    $g::nope();
    $h = Num::tryFrom($int);
    $h->nope();
    $i = Suit::ALIAS;
    $i::nope();
    $j = Plain::A;
    $j->nope();
    $k = Suit::from(...['H']);
    $l = rand(0, 1) ? 'H' : 'S';
    $m = Suit::from($l);
    $m::nope();
    $n = Suit::tryFrom(rand(0, 1) ? 'H' : 'Z');
    $n->nope();
    $o = new Holder(Suit::Hearts);
    $o->suit::nope();
    $p = [Suit::Hearts, Suit::Spades][0];
    $p->nope();
    $q = Suit::cases()[0];
    $q->nope();
    $r = Suit::Hearts;
    $r->value->nope();
    $s = static fn (): Suit => Suit::Hearts;
    $s()->nope();
    $t = match ($int) { 1 => Suit::Hearts, default => Suit::Spades };
    $t->nope();
    $u = Suit::from(value: 'S');
    $u->nope();
}

function g(Suit $s): void
{
    if ($s === Suit::Hearts) {
        $s->nope();
        return;
    }
    $s->nope2();
}

enum NoCases: string
{
}

function h(): void
{
    \PHPStan\dumpType(Suit::cases());
    \PHPStan\dumpType(NoCases::cases());
    \PHPStan\dumpType(NoCases::tryFrom('H'));
    \PHPStan\dumpType(Plain::cases());
}

final class NotAnEnum
{
    /** @return list<int> */
    public static function cases(): array
    {
        return [1];
    }
}

function k(): void
{
    \PHPStan\dumpType(NotAnEnum::cases());
}
