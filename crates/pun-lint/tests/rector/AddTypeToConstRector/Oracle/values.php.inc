<?php

namespace App\Values;

class Base
{
    public const BASE = 1;
}

enum Suit: string
{
    case Hearts = 'h';
}

class Typed
{
    public const int|string UNION = 1;
    public const int INT = 2;
}

final class Values
{
    public const LINE = __LINE__;
    public const EOL = PHP_EOL;
    public const PI = M_PI;
    public const ALL = E_ALL;
    public const SIZE = \PHP_INT_SIZE;
    public const FAMILY = PHP_OS_FAMILY;
    public const CLS = Base::class;
    public const CASE = Suit::Hearts;
    public const ENUM_VALUE = Suit::Hearts->value;
    public const UNKNOWN = UNKNOWN_CONST;
    public const FALSE = false;
    public const UPPER_TRUE = TRUE;
    public const UPPER_NULL = NULL;
    public const DOUBLE_QUOTED = "plain";
    public const NOWDOC = <<<'TXT'
        text
        TXT;
    public const HEREDOC = <<<TXT
        text
        TXT;
    public const SUM = 1 + 2;
    public const CONCAT_INTS = 1 . 2;
    public const LIST = [1, 2];
    public const OLD = array(1);
    public const NEG_FLOAT = -1.5;
    public const NEG_CONST = -PHP_INT_MAX;
    public const NEG_STRING = -'1';
    public const UNION_REF = Typed::UNION;
    public const INT_REF = Typed::INT;
    public const SELF_REF = self::LIST;
    public const PAREN = ('a');
    public const TERNARY = true ? 1 : 2;
    public const COALESCE = null ?? 1;
    public const NULL_REF = self::NUL;
    public const NUL = null;
    public const BOOL_REF = self::FALSE;
    public const EITHER = UNKNOWN_CONST ? 1 : 'x';
    public const EITHER_REF = self::EITHER;
    public const STRINGS = UNKNOWN_CONST ? 'a' : 'b';
    public const STRINGS_REF = self::STRINGS;
    public const MIXED = 'a', TYPES = 1;
    public const SAME = 'a', TYPE = 'b';
    public const NONE = UNKNOWN_CONST, MORE = UNKNOWN_CONST;
    public const SOME = UNKNOWN_CONST, OTHER = 'b';
}
-----
<?php

namespace App\Values;

class Base
{
    public const BASE = 1;
}

enum Suit: string
{
    case Hearts = 'h';
}

class Typed
{
    public const int|string UNION = 1;
    public const int INT = 2;
}

final class Values
{
    public const LINE = __LINE__;
    public const string EOL = PHP_EOL;
    public const float PI = M_PI;
    public const int ALL = E_ALL;
    public const int SIZE = \PHP_INT_SIZE;
    public const string FAMILY = PHP_OS_FAMILY;
    public const string CLS = Base::class;
    public const CASE = Suit::Hearts;
    public const ENUM_VALUE = Suit::Hearts->value;
    public const UNKNOWN = UNKNOWN_CONST;
    public const bool FALSE = false;
    public const bool UPPER_TRUE = TRUE;
    public const null UPPER_NULL = NULL;
    public const string DOUBLE_QUOTED = "plain";
    public const string NOWDOC = <<<'TXT'
        text
        TXT;
    public const string HEREDOC = <<<TXT
        text
        TXT;
    public const SUM = 1 + 2;
    public const string CONCAT_INTS = 1 . 2;
    public const array LIST = [1, 2];
    public const array OLD = array(1);
    public const float NEG_FLOAT = -1.5;
    public const int NEG_CONST = -PHP_INT_MAX;
    public const string NEG_STRING = -'1';
    public const int UNION_REF = Typed::UNION;
    public const int INT_REF = Typed::INT;
    public const array SELF_REF = self::LIST;
    public const string PAREN = ('a');
    public const TERNARY = true ? 1 : 2;
    public const COALESCE = null ?? 1;
    public const NULL_REF = self::NUL;
    public const null NUL = null;
    public const bool BOOL_REF = self::FALSE;
    public const EITHER = UNKNOWN_CONST ? 1 : 'x';
    public const EITHER_REF = self::EITHER;
    public const STRINGS = UNKNOWN_CONST ? 'a' : 'b';
    public const string STRINGS_REF = self::STRINGS;
    public const MIXED = 'a', TYPES = 1;
    public const string SAME = 'a', TYPE = 'b';
    public const NONE = UNKNOWN_CONST, MORE = UNKNOWN_CONST;
    public const SOME = UNKNOWN_CONST, OTHER = 'b';
}
