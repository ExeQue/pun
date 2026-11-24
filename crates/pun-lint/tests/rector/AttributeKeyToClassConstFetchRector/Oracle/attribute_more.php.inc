<?php

namespace App\Model;

use App\Attr\Column;
use App\Attr\Types;

#[Column(type: 'string')]
class Model
{
    #[Column(type: 'string', name: 'a')]
    public $a;

    #[Column('string')]
    public $b;

    #[Column(TYPE: 'string')]
    public $c;

    #[Column(type: 5)]
    public $d;

    #[Column(type: '5')]
    public $e;

    #[Column(type: Types::STRING)]
    public $f;

    #[Column(type: \App\Attr\Types::STRING)]
    public $g;

    #[Column(type: 'other')]
    public $h;

    #[Column(type: Types::TEXT)]
    public $i;

    public function __construct(
        #[Column(type: 'string')]
        public $j = null,
    ) {}

    #[Column(type: 'string')]
    public function k() {}
}

#[Column(type: 'string')]
function l() {}

$m = #[Column(type: 'string')] fn() => 1;
$n = #[Column(type: 'string')] function () {};

#[Column(type: 'string')]
enum O {}
-----
<?php

namespace App\Model;

use App\Attr\Column;
use App\Attr\Types;

#[Column(type: \App\Attr\Types::STRING)]
class Model
{
    #[Column(type: \App\Attr\Types::STRING, name: 'a')]
    public $a;

    #[Column('string')]
    public $b;

    #[Column(TYPE: \App\Attr\Types::STRING)]
    public $c;

    #[Column(type: \App\Attr\Types::FIVE)]
    public $d;

    #[Column(type: \App\Attr\Types::FIVE)]
    public $e;

    #[Column(type: Types::STRING)]
    public $f;

    #[Column(type: \App\Attr\Types::STRING)]
    public $g;

    #[Column(type: 'other')]
    public $h;

    #[Column(type: Types::TEXT)]
    public $i;

    public function __construct(
        #[Column(type: \App\Attr\Types::STRING)]
        public $j = null,
    ) {}

    #[Column(type: \App\Attr\Types::STRING)]
    public function k() {}
}

#[Column(type: \App\Attr\Types::STRING)]
function l() {}

$m = #[Column(type: \App\Attr\Types::STRING)] fn() => 1;
$n = #[Column(type: \App\Attr\Types::STRING)] function () {};

#[Column(type: 'string')]
enum O {}
