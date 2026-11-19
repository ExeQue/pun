<?php

class SkipConditionalReturn
{
    public function run(bool $flag): array
    {
        if ($flag) {
            return ['item'];
        }
    }
}
