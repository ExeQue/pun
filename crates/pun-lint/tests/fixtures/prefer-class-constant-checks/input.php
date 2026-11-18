<?php

namespace App;

final class User
{
}

final class Names
{
    public function strings(): array
    {
        return [
            'App\User',
            'App\User::method',
            'Throwable',
        ];
    }
}
