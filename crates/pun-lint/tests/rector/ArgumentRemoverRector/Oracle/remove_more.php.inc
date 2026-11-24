<?php

namespace App\Use;

use App\Lib\Client;

class SubClient extends Client
{
    public function send($a, $b = null, $c = null)
    {
        parent::send($a, $b, $c);
    }

    public function flag($a = null) {}

    public function byName($first = null)
    {
    }
}

function run(Client $client, SubClient $sub, $x)
{
    $client->send(1, 2, 3);
    $client->send(1);
    $client->SEND(1,
        2);
    $sub->send(1, 2);
    $client->flag(true);
    $client->flag(TRUE);
    $client->flag('yes');
    $client->flag(3);
    $client->flag(3.0);
    $client->flag(false);
    $client->flag(null);
    $client->byName(first: 1);
    $client->byName(1);
    $client->byName(FIRST: 1);
    $x->send(1, 2);
}
-----
<?php

namespace App\Use;

use App\Lib\Client;

class SubClient extends Client
{
    public function send($a)
    {
        parent::send($a);
    }

    public function flag($a = null) {}

    public function byName()
    {
    }
}

function run(Client $client, SubClient $sub, $x)
{
    $client->send(1);
    $client->send(1);
    $client->SEND(1);
    $sub->send(1);
    $client->flag();
    $client->flag();
    $client->flag();
    $client->flag();
    $client->flag(3.0);
    $client->flag(false);
    $client->flag();
    $client->byName();
    $client->byName(1);
    $client->byName();
    $x->send(1, 2);
}
