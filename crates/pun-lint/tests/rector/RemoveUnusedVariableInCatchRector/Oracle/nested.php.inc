<?php

function outer()
{
    try {
        try {
            run();
        } catch (InvalidArgumentException $first) {
        } catch (RuntimeException | LogicException $second) {
            log();
        }
    } catch (Exception $outer) {
        report($first);
    }
    if (true) {
        try {
        } catch (Throwable $e) {
        } finally {
            echo $e;
        }
        try {
        } catch (Throwable $f) {
            $g = compact('f');
        }
    }
    try {
    } catch (Throwable $later) {
    }
    echo $$name;
}
-----
<?php

function outer()
{
    try {
        try {
            run();
        } catch (InvalidArgumentException $first) {
        } catch (RuntimeException | LogicException) {
            log();
        }
    } catch (Exception $outer) {
        report($first);
    }
    if (true) {
        try {
        } catch (Throwable $e) {
        } finally {
            echo $e;
        }
        try {
        } catch (Throwable $f) {
            $g = compact('f');
        }
    }
    try {
    } catch (Throwable $later) {
    }
    echo $$name;
}
