<?php

namespace PunFacadeStaticMethods;

use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Storage;

final class Calls
{
    public function undefined(): void
    {
        Cache::doesNotExist('key');
        DB::doesNotExist('key');
        Storage::doesNotExist('key');
        Auth::doesNotExist('key');
    }

    public function declared(): void
    {
        Cache::get('key');
        DB::table('users');
        Storage::disk('local');
        Auth::check();
    }
}
