<?php

namespace App;

/**
 * @phpstan-param list<int> $items the items
 * @param array $items
 * @phpstan-return non-empty-string
 */
function run(array $items): string
{
    return 'x';
}

final class Holder
{
    /**
     * @phpstan-var non-empty-string
     */
    public const NAME = 'x';

    /**
     * Some text.
     *
     * @param array $a first
     * @param array $b second
     * @phpstan-param list<string> $b
     */
    public function two(array $a, array $b): void
    {
    }

    public function promoted(
        /** @phpstan-var list<int> */
        public array $values = [],
    ) {
    }

    /**
     * @return int
     */
    public function plain(): int
    {
        return 1;
    }

    // not a doc
    public function comment(): int
    {
        return 1;
    }
}
-----
<?php

namespace App;

/**
 * @param list<int> $items the items
 * @return non-empty-string
 */
function run(array $items): string
{
    return 'x';
}

final class Holder
{
    /**
     * @var non-empty-string
     */
    public const NAME = 'x';

    /**
     * Some text.
     *
     * @param array $a first
     * @param list<string> $b second
     */
    public function two(array $a, array $b): void
    {
    }

    public function promoted(
        /** @var list<int> */
        public array $values = [],
    ) {
    }

    /**
     * @return int
     */
    public function plain(): int
    {
        return 1;
    }

    // not a doc
    public function comment(): int
    {
        return 1;
    }
}
