<?php

namespace App;

class Builder
{
    /**
     * Adds a part.
     *
     * @return $this The builder instance (for fluid interface)
     */
    public function add()
    {
        return $this;
    }

    /** @return int The count */
    public function count()
    {
        return 'many';
    }

    /**
     * @return string|null the name, when there is one
     */
    public function name()
    {
        return 1;
    }

    /**
     * @return int|
     */
    public function dangling()
    {
        return 1;
    }

    /**
     * @return array<int>description
     */
    public function glued()
    {
        return [];
    }
}

/**
 * @return bool true on success
 */
function save()
{
    return 'yes';
}
