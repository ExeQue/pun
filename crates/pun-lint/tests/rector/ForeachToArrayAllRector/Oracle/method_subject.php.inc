<?php

namespace Rector\Tests\Php84\Rector\Foreach_\ForeachToArrayAllRector\Oracle;

final class MethodSubject
{
    /**
     * @return array<int, \DateTime>
     */
    public function items(): array
    {
        return [];
    }

    public function iterable(): iterable
    {
        return [];
    }

    public function allSet(): bool
    {
        foreach ($this->items() as $item) {
            if (! $item->getTimestamp()) {
                return false;
            }
        }

        return true;
    }

    public function skipIterable(): bool
    {
        foreach ($this->iterable() as $item) {
            if (! $item) {
                return false;
            }
        }

        return true;
    }
}
-----
<?php

namespace Rector\Tests\Php84\Rector\Foreach_\ForeachToArrayAllRector\Oracle;

final class MethodSubject
{
    /**
     * @return array<int, \DateTime>
     */
    public function items(): array
    {
        return [];
    }

    public function iterable(): iterable
    {
        return [];
    }

    public function allSet(): bool
    {
        return array_all($this->items(), fn($item) => $item->getTimestamp());
    }

    public function skipIterable(): bool
    {
        foreach ($this->iterable() as $item) {
            if (! $item) {
                return false;
            }
        }

        return true;
    }
}
