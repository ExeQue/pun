<?php

namespace App;

class Connect
{
    public function run($config)
    {
        $a = ldap_connect("ldap://ldap.example.com", 389);
        $b = ldap_connect('ldap://it\'s\\here', 636);
        $c = \ldap_connect($config['host'], $this->port);
        $d = ldap_connect($this->host(), $config->port);
        $e = ldap_connect($config?->host, $config?->port());
    }
}
-----
<?php

namespace App;

class Connect
{
    public function run($config)
    {
        $a = ldap_connect('ldap://ldap.example.com:389');
        $b = ldap_connect('ldap://it\'s\here:636');
        $c = \ldap_connect("{$config['host']}:{$this->port}");
        $d = ldap_connect("{$this->host()}:{$config->port}");
        $e = ldap_connect("{$config?->host}:{$config?->port()}");
    }
}
