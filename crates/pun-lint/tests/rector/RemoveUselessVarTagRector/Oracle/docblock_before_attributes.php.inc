<?php

namespace App;

#[\Attribute]
final class DocblockMarker
{
}

final class DocblockBeforeAttributes
{
    /** @var int */
    #[DocblockMarker]
    public int $count = 0;

    /** @var int */
    #[DocblockMarker]
    #[\Deprecated]
    public int $twice = 0;

    /**
     * @var string
     */
    #[DocblockMarker]
    #[\Deprecated]
    public const string NAME = 'a';

    /** @var string */
    #[DocblockMarker]
    final public const string OTHER = 'b';

    /** @var int */
    public int $plain = 0;

    /** @var int */
    #[DocblockMarker]
    public int $hooked {
        get => 1;
    }
}
-----
<?php

namespace App;

#[\Attribute]
final class DocblockMarker
{
}

final class DocblockBeforeAttributes
{
    #[DocblockMarker]
    public int $count = 0;

    #[DocblockMarker]
    #[\Deprecated]
    public int $twice = 0;

    #[DocblockMarker]
    #[\Deprecated]
    public const string NAME = 'a';

    #[DocblockMarker]
    final public const string OTHER = 'b';

    public int $plain = 0;

    #[DocblockMarker]
    public int $hooked {
        get => 1;
    }
}
