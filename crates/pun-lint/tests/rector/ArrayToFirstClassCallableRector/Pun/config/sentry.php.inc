<?php

final class SanitizeEvent
{
    public static function sanitize(mixed $event): mixed
    {
        return $event;
    }
}

return [
    'before_send' => [SanitizeEvent::class, 'sanitize'],
    'before_send_transaction' => [SanitizeEvent::class, 'sanitize'],
    'resolver' => function () {
        return [SanitizeEvent::class, 'sanitize'];
    },
];
-----
<?php

final class SanitizeEvent
{
    public static function sanitize(mixed $event): mixed
    {
        return $event;
    }
}

return [
    'before_send' => [SanitizeEvent::class, 'sanitize'],
    'before_send_transaction' => [SanitizeEvent::class, 'sanitize'],
    'resolver' => function () {
        return SanitizeEvent::sanitize(...);
    },
];
