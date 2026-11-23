<?php

namespace App;

final class Narrow
{
    /**
     * @return bool|null
     */
    public function nullable(): false|null
    {
        return null;
    }

    /**
     * @return bool
     */
    public function notUnion(): false|int
    {
        return false;
    }

    /**
     * @return bool|int
     */
    public function native(): ?int
    {
        return 1;
    }

    /**
     * @return bool|int
     */
    public function noNative()
    {
        return 1;
    }

    /**
     * @return array<int>|bool|string described
     */
    public function described(): string|array|true
    {
        return true;
    }

    /**
     * @return bool|bool[]
     */
    public function twice(): false|array
    {
        return false;
    }
}
-----
<?php

namespace App;

final class Narrow
{
    /**
     * @return false|null
     */
    public function nullable(): false|null
    {
        return null;
    }

    /**
     * @return bool
     */
    public function notUnion(): false|int
    {
        return false;
    }

    /**
     * @return bool|int
     */
    public function native(): ?int
    {
        return 1;
    }

    /**
     * @return bool|int
     */
    public function noNative()
    {
        return 1;
    }

    /**
     * @return array<int>|true|string described
     */
    public function described(): string|array|true
    {
        return true;
    }

    /**
     * @return false|bool[]
     */
    public function twice(): false|array
    {
        return false;
    }
}
