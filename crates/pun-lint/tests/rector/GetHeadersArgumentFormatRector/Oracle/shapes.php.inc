<?php

function headers($url)
{
    get_headers($url, (1));
    get_headers($url, format: 0);
    get_headers(url: $url, associative: 1);
    get_headers($url, 2);
    get_headers($url, 0x1);
    version_compare('1', '2', "gte");
    version_compare('1', '2', ('n'));
    version_compare('1', '2', 'GTE');
    version_compare(version1: '1', version2: '2', operator: 'l');
}
-----
<?php

function headers($url)
{
    get_headers($url, (true));
    get_headers($url, format: false);
    get_headers(url: $url, associative: true);
    get_headers($url, 2);
    get_headers($url, true);
    version_compare('1', '2', "gte");
    version_compare('1', '2', ('n'));
    version_compare('1', '2', 'GTE');
    version_compare(version1: '1', version2: '2', operator: 'l');
}
