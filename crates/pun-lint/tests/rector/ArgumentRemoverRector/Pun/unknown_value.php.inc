<?php

namespace App\Use;

use App\Lib\Client;

function run(Client $client, $x)
{
    $client->flag($x);
    $client->flag(strtoupper($x));
}
