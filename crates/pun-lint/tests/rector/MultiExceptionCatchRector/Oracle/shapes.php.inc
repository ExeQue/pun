<?php

function run()
{
    try {
        foo();
    } catch (A $e) {
        log($e);
    } catch (B|C $other) {
        log($e);
    } catch (D) {
        log($e);
    } catch (E $e) {
        other();
    } catch (F $e) {
        other();
    }

    try {
        foo();
    } catch (A $e) {
        // one
        bar();
    } catch (B $e) {
        // two
        bar();
    }

    try {
        foo();
    } catch (A $e) {
    } catch (B $e) {
    }
}
-----
<?php

function run()
{
    try {
        foo();
    } catch (A|B|C|D $e) {
        log($e);
    } catch (E|F $e) {
        other();
    }

    try {
        foo();
    } catch (A $e) {
        // one
        bar();
    } catch (B $e) {
        // two
        bar();
    }

    try {
        foo();
    } catch (A|B $e) {
    }
}
