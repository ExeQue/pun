<?php

namespace App;

class SomeJob
{
}

class QueuedJob implements \Illuminate\Contracts\Queue\ShouldQueue
{
}

function run($unknown, SomeJob $job, QueuedJob $queued)
{
    dispatch($unknown);
    dispatch($job);
    dispatch($queued);
    dispatch(new SomeJob(), 'extra');
    \Illuminate\Support\Facades\Bus::dispatch($job);
    \Illuminate\Support\Facades\Bus::dispatch($queued);
}
-----
<?php

namespace App;

class SomeJob
{
}

class QueuedJob implements \Illuminate\Contracts\Queue\ShouldQueue
{
}

function run($unknown, SomeJob $job, QueuedJob $queued)
{
    dispatch($unknown);
    dispatch_sync($job);
    dispatch($queued);
    dispatch(new SomeJob(), 'extra');
    \Illuminate\Support\Facades\Bus::dispatchSync($job);
    \Illuminate\Support\Facades\Bus::dispatch($queued);
}
