<?php

namespace DeprecationsExtra5;

trait Labels
{
	public string $labelText = '';

	public static string $staticLabel = '';

	public function label(): static
	{
		return $this;
	}

	public static function staticLabel(): string
	{
		return '';
	}
}

/** @deprecated Use Column instead. */
class Fluent
{
	use Labels;
}

class Column
{
	use Labels;
}

function uses(Fluent $f, Column $c): void
{
	$f->label();
	echo $f->labelText;
	echo Fluent::$staticLabel;
	echo Fluent::staticLabel();
	$c->label();
	echo $c->labelText;
}
