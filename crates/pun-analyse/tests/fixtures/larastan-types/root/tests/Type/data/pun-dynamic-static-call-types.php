<?php

namespace PunDynamicStaticCallTypes;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Collection;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Database\Eloquent\Relations\Relation;
use Illuminate\Http\Request;

/** @extends Builder<Post> */
class PostBuilder extends Builder
{
    public function published(): static
    {
        return $this;
    }
}

class Post extends Model
{
    /** @param \Illuminate\Database\Query\Builder $query */
    public function newEloquentBuilder($query): PostBuilder
    {
        return new PostBuilder($query);
    }
}

class Comment extends Model
{
}

class Author extends Model
{
    /** @return HasMany<Post, $this> */
    public function posts(): HasMany
    {
        return $this->hasMany(Post::class);
    }

    /** @return HasMany<Comment, $this> */
    public function comments(): HasMany
    {
        return $this->hasMany(Comment::class);
    }
}

/** @param Collection<int, Author> $authors */
function closureParameters(Collection $authors): void
{
    // loadExists()'s callable(Relation<*, *, *>) narrows the native `Builder|Relation`
    $authors->loadExists(['posts' => function (Builder|Relation $query): void {
        $query->whereKey(1);
    }]);
}

function relationCalls(Author $author, ?Author $maybe, bool $flag): void
{
    // the callback of a call forwarded through a relation has the builder as `static`
    $author->comments()->where(fn (Builder $query): Builder => $query->orWhereNotNull('body'));

    // a call after a nullsafe link has a nullable receiver
    $maybe?->comments()->where('id', 1)->value('body');

    // when() resolving to the relation's own builder returns the relation
    $author->posts()->when($flag, fn (PostBuilder $query): PostBuilder => $query->published())->published()->latest();
}

function storedClosure(): void
{
    // a closure sent to `(Closure(static): mixed)|string|array` is not inferred: a string may be callable
    $scope = fn (Builder $query): Builder => $query->whereNull('author_id');
    Post::query()->where($scope)->first();
}

function signedRequest(Request $request): bool
{
    // the hasValidSignature() macro, not Request's `@method` tag
    return $request->hasValidSignature();
}
