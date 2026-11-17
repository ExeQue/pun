<?php

namespace OverridingPropertyPhpDocCoverage;

/** @template T of int|string */
abstract class GenericBase
{
	/** @var T */
	protected $value;

	/** @var array<int, T> */
	protected $items = [];
}

/** @extends GenericBase<int> */
final class IntChild extends GenericBase
{
	/** @var int */
	protected $value = 1;

	/** @var array<string, int> */
	protected $items = [];
}

final class RawChild extends GenericBase
{
	/** @var float */
	protected $value = 1.0;
}

trait Guards
{
	/** @var array<string> */
	protected $guarded = ['*'];
}

class Model
{
	use Guards;
}

final class Pivot extends Model
{
	/** @var array<string>|bool */
	protected $guarded = [];
}

class Base
{
	/** @var list<string> */
	protected array $hidden = [];

	protected $plain;
}

final class Promoted extends Base
{
	/** @param array<string> $hidden */
	public function __construct(protected array $hidden = [])
	{
	}
}

final class Plain extends Base
{
	/** @var int */
	protected $plain;
}

/** @template TItem */
trait HasItems
{
	/** @var list<TItem> */
	protected array $items = [];
}

class Owner
{
	/** @use HasItems<\Countable> */
	use HasItems;
}

final class SubOwner extends Owner
{
	/** @use HasItems<\ArrayObject<int, int>> */
	use HasItems;
}
