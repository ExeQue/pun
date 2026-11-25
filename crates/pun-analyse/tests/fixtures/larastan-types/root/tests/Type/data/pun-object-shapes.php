<?php

namespace PunObjectShapes;

use Illuminate\Database\Eloquent\Model;

use function PHPStan\Testing\assertType;

/** @property string $id */
class Post extends Model {}

class NoTag extends Model {}

/** @property int $id */
class IntTag extends Model {}

class Account extends Model
{
    protected $table = 'users';
}

/** @property string $id */
final class Magic
{
    public function __get(string $name): mixed
    {
        return '';
    }
}

/** @property string $id */
final class Tagged {}

/** @property string $id */
class Open {}

class Native
{
    public string $id = '';
}

class Hidden
{
    protected string $id = '';
}

class Shared
{
    public static string $id = '';
}

final class Uses
{
    /** @param object{id: string} $o */
    public function shape(object $o): void {}

    /** @param object{name: string} $o */
    public function named(object $o): void {}

    /**
     * @template T of object{id: string}
     * @param T $o
     * @return T
     */
    public function bounded(object $o): object
    {
        return $o;
    }

    /**
     * @template T of object{name: string}
     * @param T $o
     * @return T
     */
    public function boundedName(object $o): object
    {
        return $o;
    }

    /**
     * @template TModel of Model&object{id: string}
     * @param TModel $m
     * @return TModel
     */
    public function model(Model $m): Model
    {
        return $m;
    }

    public function run(): void
    {
        $this->shape(new Post());
        $this->shape(new NoTag());
        $this->shape(new IntTag());
        $this->shape(new Magic());
        $this->shape(new Tagged());
        $this->shape(new Open());
        $this->shape(new Native());
        $this->shape(new Hidden());
        $this->shape(new Shared());
        $this->named(new Account());
        $this->named(new NoTag());
        assertType('PunObjectShapes\Native', $this->bounded(new Native()));
        assertType('PunObjectShapes\Post', $this->bounded(new Post()));
        assertType('PunObjectShapes\Magic', $this->bounded(new Magic()));
        assertType('object{id: string}', $this->bounded(new Tagged()));
        assertType('object{id: string}', $this->bounded(new Shared()));
        assertType('PunObjectShapes\Account', $this->boundedName(new Account()));
        assertType('PunObjectShapes\Post', $this->model(new Post()));
        assertType('Illuminate\Database\Eloquent\Model&object{id: string}', $this->model(new Account()));
    }
}
