<?php

namespace BuiltinThrowsInheritance;

class InheritsThrows extends \Redis
{
    public function connect($host, $port = 6379, $timeout = 0, $persistent_id = null, $retry_interval = 0, $read_timeout = 0, $context = null): bool
    {
        return true;
    }
}

class WidensThrows extends \Redis
{
    /** @throws \LogicException */
    public function pconnect($host, $port = 6379, $timeout = 0, $persistent_id = null, $retry_interval = 0, $read_timeout = 0, $context = null): bool
    {
        return true;
    }
}

class InheritsClusterThrows extends \RedisCluster
{
    public function acl($key_or_address, $subcmd, ...$args): mixed
    {
        return null;
    }
}
