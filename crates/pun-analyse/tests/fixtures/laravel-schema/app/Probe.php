<?php

namespace App;

use App\Models;
use Illuminate\Support\Str;

use function PHPStan\dumpType;

final class Probe
{
    public function widget(Models\Widget $widget): void
    {
        dumpType($widget->id);
        dumpType($widget->tiny);
        dumpType($widget->small);
        dumpType($widget->medium);
        dumpType($widget->whole);
        dumpType($widget->real_value);
        dumpType($widget->dbl);
        dumpType($widget->flt);
        dumpType($widget->dec_value);
        dumpType($widget->num);
        dumpType($widget->fixed_value);
        dumpType($widget->bits);
        dumpType($widget->bin);
        dumpType($widget->varbin);
        dumpType($widget->code);
        dumpType($widget->name);
        dumpType($widget->alias);
        dumpType($widget->body);
        dumpType($widget->summary);
        dumpType($widget->notes);
        dumpType($widget->plain);
        dumpType($widget->status);
        dumpType($widget->level);
        dumpType($widget->sizes);
        dumpType($widget->born_on);
        dumpType($widget->year_value);
        dumpType($widget->starts_at);
        dumpType($widget->seen_at);
        dumpType($widget->happened_at);
        dumpType($widget->payload);
        dumpType($widget->geo);
        dumpType($widget->flag);
        dumpType($widget->active);
        dumpType($widget->data);
        dumpType($widget->small_data);
        dumpType($widget->medium_data);
        dumpType($widget->long_data);
    }

    /** @param array{Models\Gadget, Models\Part, Models\QualifiedThing, Models\TempThing, Models\LowerThing, Models\DupThing, Models\LaterThing, Models\EmptyThing, Models\Like, Models\NestedThing, Models\TextThing} $m */
    public function tables(array $m): void
    {
        dumpType($m[0]->name);
        dumpType($m[0]->color);
        dumpType($m[0]->secret);
        dumpType($m[1]->name);
        dumpType($m[1]->legacy_code);
        dumpType($m[2]->count);
        dumpType($m[3]->count);
        dumpType($m[4]->count);
        dumpType($m[5]->first);
        dumpType($m[5]->second);
        dumpType($m[5]->third);
        dumpType($m[6]->count);
        dumpType($m[7]->id);
        dumpType($m[8]->id);
        dumpType($m[9]->count);
        dumpType($m[10]->count);
    }

    /** @param array{Models\FailOne, Models\FailTwo, Models\FailThree, Models\FailFour, Models\FailFive} $m */
    public function failures(array $m): void
    {
        dumpType($m[0]->count);
        dumpType($m[1]->count);
        dumpType($m[2]->count);
        dumpType($m[3]->count);
        dumpType($m[4]->count);
    }

    public function macros(): void
    {
        dumpType(Str::shout('a'));
    }

    /** @param array{Models\OddThing, Models\OpenThing} $m */
    public function oddEnums(array $m): void
    {
        dumpType($m[0]->bare);
        dumpType($m[0]->empty_list);
        dumpType($m[0]->count);
        dumpType($m[1]->count);
        dumpType($m[1]->open);
    }
}
