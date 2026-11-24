<?php

namespace App;

use Exception;

class Handlers
{
    public function handleException(Exception $exception)
    {
    }

    public function onError(Exception $exception)
    {
    }

    public static function exceptionHANDLER(?Exception $exception = null)
    {
    }

    public function handle(Exception $exception, $other)
    {
    }

    public function handleUnion(Exception|\Error $exception)
    {
    }

    public function handleRuntime(\RuntimeException $exception)
    {
    }
}

function handler(\Exception $e)
{
}
-----
<?php

namespace App;

use Exception;

class Handlers
{
    public function handleException(\Throwable $exception)
    {
    }

    public function onError(Exception $exception)
    {
    }

    public static function exceptionHANDLER(?\Throwable $exception = null)
    {
    }

    public function handle(Exception $exception, $other)
    {
    }

    public function handleUnion(Exception|\Error $exception)
    {
    }

    public function handleRuntime(\RuntimeException $exception)
    {
    }
}

function handler(\Throwable $e)
{
}
