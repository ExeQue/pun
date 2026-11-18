<?php

namespace App;

class Connect
{
    public function run($host, $port)
    {
        $a = ldap_connect($host, 389);
        $b = ldap_connect('ldap://ldap.example.com', $port);
        $c = ldap_connect('ldap://ldap.example.com', 99999999999999999999);
        $d = ldap_connect(HOST, PORT);
        $e = ldap_connect($host, $port, $host);
        $f = ldap_connect(self::HOST, $port);
    }
}
