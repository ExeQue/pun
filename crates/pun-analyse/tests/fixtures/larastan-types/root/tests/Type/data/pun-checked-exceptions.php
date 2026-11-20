<?php

declare(strict_types=1);

namespace PunCheckedExceptions;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Storage;
use RuntimeException;

final class Denied extends RuntimeException
{
}

final class Post extends Model
{
}

final class Calls
{
    public function closureException(bool $blocked): void
    {
        throw_if($blocked, fn (): Denied => new Denied('no'));
    }

    public function namedException(bool $blocked): void
    {
        throw_unless($blocked, Denied::class);
    }

    public function forwardedToTheQueryBuilder(): int
    {
        return Post::query()->where('id', 1)->count();
    }

    public function storageDisk(): bool
    {
        return Storage::disk()->exists('a');
    }

    public function cacheMixin(): bool
    {
        return Cache::has('a');
    }

    public function closureVariable(int $x): int
    {
        $check = fn (int $value): int => $value > 0 ? $value : throw new Denied('negative');
        $twice = fn (int $value): int => $check($value) * 2;

        return $twice($x);
    }

    public function closureVariableWithoutThrows(int $x): int
    {
        $double = fn (int $value): int => $value * 2;

        return $double($x);
    }
}
