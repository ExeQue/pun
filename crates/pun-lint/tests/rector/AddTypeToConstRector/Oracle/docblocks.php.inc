<?php

namespace App\Docblocks;

final class Docs
{
    /** @var int Some description */
    private const A = 1;

    /**
     * Leading text.
     *
     * @var int
     */
    private const B = 1;

    /** @var non-empty-string */
    private const C = 'c';

    /** @var string */
    private const D = 1;

    /** @var int */
    private const E = 1, F = 2;

    // @var int
    private const G = 1;

    /** @var positive-int */
    private const H = 1;

    /** @var 'x' */
    private const I = 'x';

    /** @var int|string */
    private const J = 1;

    /** @var array<int> */
    private const K = [1];

    /** @var list<int> */
    private const L = [1];

    /** @var null */
    private const M = null;

    /** @deprecated */
    private const N = 1;

    /**
     * @var int
     * @deprecated
     */
    private const O = 1;

    #[\Deprecated]
    private const P = 1;

    /** @var int */
    #[\Deprecated]
    private const Q = 1;

    /** @var int */
    private const R = UNKNOWN_CONST;
}
-----
<?php

namespace App\Docblocks;

final class Docs
{
    /** @var int Some description */
    private const int A = 1;

    /**
     * Leading text.
     */
    private const int B = 1;

    /** @var non-empty-string */
    private const string C = 'c';

    private const int D = 1;

    private const int E = 1, F = 2;

    // @var int
    private const int G = 1;

    /** @var positive-int */
    private const int H = 1;

    /** @var 'x' */
    private const string I = 'x';

    private const int J = 1;

    /** @var array<int> */
    private const array K = [1];

    /** @var list<int> */
    private const array L = [1];

    private const null M = null;

    /** @deprecated */
    private const int N = 1;

    /**
     * @deprecated
     */
    private const int O = 1;

    #[\Deprecated]
    private const int P = 1;

    #[\Deprecated]
    private const int Q = 1;

    /** @var int */
    private const R = UNKNOWN_CONST;
}
