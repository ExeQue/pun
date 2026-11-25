<?php

namespace PunGenericScopeClosures;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

final class Note extends Model
{
}

final class Post extends Model
{
    /** @return HasMany<Note, $this> */
    public function notes(): HasMany
    {
        return $this->hasMany(Note::class);
    }

    /**
     * @template TModel of Model
     *
     * @param  Builder<TModel>  $query
     * @return Builder<TModel>
     */
    public function scopeAnyModel(Builder $query): Builder
    {
        $query->where(fn (Builder $q): Builder => $q);
        $query->whereHas('notes', fn ($notes) => $notes);
        $query->where(fn (int $q) => 1);
        $query->whereHas('notes', fn (int $notes) => 1);

        return $query->whereHas('notes', fn (Builder $notes): Builder => $notes->whereNotNull('body'));
    }

    /**
     * @template TModel of Post
     *
     * @param  Builder<TModel>  $query
     */
    public function scopeAnyPost(Builder $query): void
    {
        $query->whereHas('notes', fn (Builder $notes): Builder => $notes);
        $query->orWhereHas('notes', fn (Builder $notes): Builder => $notes);
        $query->whereHas('notes', fn (int $notes) => 1);
    }

    /** @param  Builder<self>  $query */
    public function scopeConcrete(Builder $query): void
    {
        $query->whereHas('notes', fn (Builder $notes): Builder => $notes->whereNotNull('body'));
    }
}
