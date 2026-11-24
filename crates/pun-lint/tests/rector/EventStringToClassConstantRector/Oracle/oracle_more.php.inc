<?php

namespace App;

event(['auth.login', 'other']);
event('auth.logout', ['payload']);
\Illuminate\Support\Facades\Event::until('auth.login');
\Illuminate\Support\Facades\Event::listen(['key' => 'auth.login'], function () {});
\Illuminate\Support\Facades\Event::other('auth.login');
-----
<?php

namespace App;

event([\Illuminate\Auth\Events\Login::class, 'other']);
event(\Illuminate\Auth\Events\Logout::class, ['payload']);
\Illuminate\Support\Facades\Event::until(\Illuminate\Auth\Events\Login::class);
\Illuminate\Support\Facades\Event::listen(['key' => 'auth.login'], function () {});
\Illuminate\Support\Facades\Event::other('auth.login');
