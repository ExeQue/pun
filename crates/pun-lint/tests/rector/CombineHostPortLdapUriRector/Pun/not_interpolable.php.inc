<?php

namespace App;

class Connect
{
    public function run($config)
    {
        $a = ldap_connect(host(), $config->port);
        $b = ldap_connect($config->scheme . $config->host, $config->port . '');
    }
}
-----
<?php

namespace App;

class Connect
{
    public function run($config)
    {
        $a = ldap_connect(host() . ':' . $config->port);
        $b = ldap_connect($config->scheme . $config->host . ':' . ($config->port . ''));
    }
}
