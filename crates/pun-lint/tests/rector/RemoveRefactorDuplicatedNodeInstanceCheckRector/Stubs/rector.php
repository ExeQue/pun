<?php

// The nikic/php-parser 5, Rector 2.8.4 and symplify/rule-doc-generator classes the fixtures name, declared as upstream
// declares them (members trimmed).

namespace PhpParser {
    interface Node
    {
    }

    abstract class NodeAbstract implements Node
    {
    }
}

namespace PhpParser\Node {
    abstract class Expr extends \PhpParser\NodeAbstract
    {
    }

    abstract class Stmt extends \PhpParser\NodeAbstract
    {
    }
}

namespace PhpParser\Node\Expr {
    class MethodCall extends \PhpParser\Node\Expr
    {
    }

    class StaticCall extends \PhpParser\Node\Expr
    {
    }

    abstract class BinaryOp extends \PhpParser\Node\Expr
    {
    }
}

namespace PhpParser\Node\Expr\BinaryOp {
    class Identical extends \PhpParser\Node\Expr\BinaryOp
    {
    }

    class NotIdentical extends \PhpParser\Node\Expr\BinaryOp
    {
    }
}

namespace PhpParser\Node\Stmt {
    class ClassMethod extends \PhpParser\Node\Stmt
    {
    }

    class Function_ extends \PhpParser\Node\Stmt
    {
    }
}

namespace Rector\Contract\PhpParser\Node {
    interface StmtsAwareInterface
    {
    }
}

namespace Rector\Rector {
    abstract class AbstractRector
    {
        abstract public function getNodeTypes(): array;

        abstract public function refactor(\PhpParser\Node $node);
    }
}

namespace Symplify\RuleDocGenerator\ValueObject {
    final class RuleDefinition
    {
    }
}
