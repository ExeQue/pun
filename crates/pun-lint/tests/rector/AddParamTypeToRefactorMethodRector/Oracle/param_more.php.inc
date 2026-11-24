<?php

namespace App\Rules;

use App\Missing;
use PhpParser\Node;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Stmt\ClassMethod;
use Rector\Rector\AbstractRector;

final class Reversed extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [StaticCall::class, MethodCall::class];
    }

    public function refactor(Node $node): ?Node
    {
    }
}

final class WithText extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [ClassMethod::class];
    }

    /**
     * Some text.
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class Duplicated extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, MethodCall::class];
    }

    public function refactor(Node $node): ?Node
    {
    }
}

final class OtherParam extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    /**
     * @param MethodCall $other
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class NoParam extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor()
    {
    }
}

final class NotAnArray extends AbstractRector
{
    private const TYPES = [MethodCall::class];

    public function getNodeTypes(): array
    {
        return self::TYPES;
    }

    public function refactor(Node $node): ?Node
    {
    }
}

final class Unknown extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [Missing::class];
    }

    public function refactor(Node $node): ?Node
    {
    }
}
-----
<?php

namespace App\Rules;

use App\Missing;
use PhpParser\Node;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Stmt\ClassMethod;
use Rector\Rector\AbstractRector;

final class Reversed extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [StaticCall::class, MethodCall::class];
    }

    /**
     * @param MethodCall|StaticCall $node
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class WithText extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [ClassMethod::class];
    }

    /**
     * Some text.
     * @param ClassMethod $node
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class Duplicated extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, MethodCall::class];
    }

    /**
     * @param MethodCall $node
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class OtherParam extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    /**
     * @param MethodCall $other
     * @param MethodCall $node
     */
    public function refactor(Node $node): ?Node
    {
    }
}

final class NoParam extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor()
    {
    }
}

final class NotAnArray extends AbstractRector
{
    private const TYPES = [MethodCall::class];

    public function getNodeTypes(): array
    {
        return self::TYPES;
    }

    public function refactor(Node $node): ?Node
    {
    }
}

final class Unknown extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [Missing::class];
    }

    public function refactor(Node $node): ?Node
    {
    }
}
