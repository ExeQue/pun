<?php

namespace App\Rules;

use PhpParser\Node;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\StaticCall;
use Rector\Rector\AbstractRector;

final class ParamWins extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, StaticCall::class];
    }

    /**
     * @param StaticCall $node
     */
    public function refactor(Node $node)
    {
        if (! $node instanceof StaticCall) {
            return null;
        }

        return $node;
    }
}

final class ParamDiffers extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    /**
     * @param StaticCall $node
     */
    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class TwoTypes extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, StaticCall::class];
    }

    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class NotFirst extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        $a = 1;
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class NotNegated extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        if ($node instanceof MethodCall) {
            return null;
        }
    }
}

class NotRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}
-----
<?php

namespace App\Rules;

use PhpParser\Node;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\StaticCall;
use Rector\Rector\AbstractRector;

final class ParamWins extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, StaticCall::class];
    }

    /**
     * @param StaticCall $node
     */
    public function refactor(Node $node)
    {
        return $node;
    }
}

final class ParamDiffers extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    /**
     * @param StaticCall $node
     */
    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class TwoTypes extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class, StaticCall::class];
    }

    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class NotFirst extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        $a = 1;
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}

final class NotNegated extends AbstractRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        if ($node instanceof MethodCall) {
            return null;
        }
    }
}

class NotRector
{
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    public function refactor(Node $node)
    {
        if (! $node instanceof MethodCall) {
            return null;
        }
    }
}
