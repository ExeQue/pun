<?php

namespace RectorLaravel\Tests\Rector\StaticCall\RouteActionCallableRector\Source;

/**
 * @method void magic()
 */
class MoreController extends MoreBaseController
{
    public function index() {}
}

class MoreBaseController
{
    public function inherited() {}
}
