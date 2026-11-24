<?php

namespace App\Service;

class Mailer
{
}

/**
 * @see https://example.com
 */
final class Sender
{
}

interface Port
{
}

/** @see \App\Tests\Service\Missing */
final class Other
{
}

final class OtherTest
{
}
-----
<?php

namespace App\Service;

/**
 * @see \App\Tests\Service\MailerTest
 */
class Mailer
{
}

/**
 * @see https://example.com
 * @see \App\Tests\Service\SenderTest
 */
final class Sender
{
}

interface Port
{
}

/**
 * @see \App\Tests\Service\Missing
 * @see \App\Service\OtherTest
 */
final class Other
{
}

final class OtherTest
{
}
