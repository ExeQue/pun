<?php

namespace App;

trait Inner
{
}

trait Nested
{
    use Inner;
}
