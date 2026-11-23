<?php

class Vars
{
    public $HTTP_GET_VARS;
    public static $HTTP_POST_VARS;

    public function run()
    {
        echo $HTTP_SERVER_VARS['x'], $HTTP_GET_VARS, $http_post_vars, $HTTP_COOKIE_VARS, ${'HTTP_ENV_VARS'}, $HTTP_RAW_POST_DATA;
        echo $this->HTTP_GET_VARS, self::$HTTP_POST_VARS, $HTTP_POST_FILES, "$HTTP_ENV_VARS";
    }
}
-----
<?php

class Vars
{
    public $HTTP_GET_VARS;
    public static $HTTP_POST_VARS;

    public function run()
    {
        echo $_SERVER['x'], $_GET, $http_post_vars, $_COOKIE, ${'HTTP_ENV_VARS'}, $HTTP_RAW_POST_DATA;
        echo $this->HTTP_GET_VARS, self::$HTTP_POST_VARS, $_FILES, "$_ENV";
    }
}
