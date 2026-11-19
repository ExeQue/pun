<?php

namespace PunDynamicStaticCalls;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Database\Eloquent\SoftDeletes;

/** @extends Builder<Post> */
class PostBuilder extends Builder
{
    public function published(): static
    {
        return $this->whereNotNull('published_at');
    }
}

/**
 * @template TModel of Model
 * @extends Builder<TModel>
 */
class GenericBuilder extends Builder
{
    public function active(): static
    {
        return $this->whereIn('active', [1]);
    }
}

class Post extends Model
{
    use SoftDeletes;

    /** @param \Illuminate\Database\Query\Builder $query */
    public function newEloquentBuilder($query): PostBuilder
    {
        return new PostBuilder($query);
    }

    public function title(): string
    {
        return 'title';
    }
}

class Comment extends Model
{
    /** @param Builder<Comment> $query */
    public function scopeRecent(Builder $query): void
    {
        $query->latest();
    }
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

/** @mixin Post */
class PlainProxy
{
    /** @param array<mixed> $arguments */
    public function __call(string $method, array $arguments): mixed
    {
        return null;
    }
}

/** @mixin Post */
class StaticProxy
{
    /** @param array<mixed> $arguments */
    public function __call(string $method, array $arguments): mixed
    {
        return null;
    }

    /** @param array<mixed> $arguments */
    public static function __callStatic(string $method, array $arguments): mixed
    {
        return null;
    }
}

/**
 * @param  Builder<Comment>  $comments
 * @param  Builder<Comment>|null  $maybe
 * @param  Builder<Comment>|PostBuilder  $either
 * @param  \Illuminate\Support\Collection<int, Post>  $list
 */
function calls(Builder $comments, ?Builder $maybe, Builder $either, PostBuilder $posts, Post $post, Author $author, PlainProxy $plain, StaticProxy $static, \Illuminate\Support\Collection $list, Builder $raw): void
{
    $a = $comments->whereIn('id', [1]);
    $b = $comments->where('id', 1);
    $c = $comments->whereTitle('x');
    $d = $comments->recent();
    $e = $posts->whereIn('id', [1]);
    $f = $posts->published();
    $g = $posts->withTrashed();
    $h = $post->published();
    $i = $post->where('id', 1);
    $j = $post->restore();
    $k = $author->posts()->whereIn('id', [1]);
    $l = $author->posts()->where('id', 1);
    $m = $plain->title();
    $n = $static->title();
    $o = $either->whereIn('id', [1]);
    $p = $either->where('id', 1);
    $q = $maybe?->whereIn('id', [1]);
    $r = $post->title();
    $s = $post->whereTitle('x');
    $t = $author->comments()->recent();
    $u = $list->map->whereTitle('x');
    $v = $raw->whereIn('id', [1]);
}
