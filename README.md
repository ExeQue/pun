<p align="center"><img src="docs/public/og.png" width="600" alt="Pun: ultrafast unified PHP tooling"></p>

<p align="center">
<a href="https://packagist.org/packages/punphp/pun"><img src="https://img.shields.io/packagist/dt/punphp/pun" alt="Total Downloads"></a>
<a href="https://packagist.org/packages/punphp/pun"><img src="https://img.shields.io/packagist/v/punphp/pun" alt="Latest Stable Version"></a>
<a href="https://packagist.org/packages/punphp/pun"><img src="https://img.shields.io/packagist/l/punphp/pun" alt="License"></a>
</p>

## Introduction

Pun is one native binary for projects that target PHP 8.3 or later. It formats, lints, and analyses your code, and installs your Composer packages from `composer.json` and `composer.lock`. Its own settings live in one `pun.toml`. It formats code byte for byte like [Laravel Pint](https://github.com/laravel/pint), and `pun migrate` converts your existing Pint, PHP-CS-Fixer, Rector, and PHPStan configs.

Install the binary on Linux or macOS (x86_64 or arm64):

```sh
curl -fsSL https://pun.laravel.cloud/install | sh
```

Or add it to a project with Composer:

```sh
composer require --dev punphp/pun

vendor/bin/pun migrate   # or: vendor/bin/pun init
vendor/bin/pun check --fix
```

## Official Documentation

The documentation is on the [Pun website](https://pun.laravel.cloud/docs).

## Contributing

Thank you for considering contributing to Pun! [AGENTS.md](AGENTS.md) explains how to build Pun (Rust 1.100 or later), which checks to run, and which upstream tools Pun is tested against. If Pun is missing a rule you rely on, open an issue or send a pull request.

## Security Vulnerabilities

To report a security vulnerability, follow [our security policy](https://github.com/shipfastlabs/pun/security/policy).

## Credits

Pun would not be possible without [Mago](https://github.com/carthage-software/mago), [PHP-CS-Fixer](https://github.com/PHP-CS-Fixer/PHP-CS-Fixer), [Laravel Pint](https://github.com/laravel/pint), [PHPStan](https://github.com/phpstan/phpstan), [Larastan](https://github.com/larastan/larastan), [Rector](https://github.com/rectorphp/rector) and [Composer](https://github.com/composer/composer). Pun uses Mago's parser and analysis libraries directly and ports behavior, rules and test fixtures from these projects. Thank you to everyone behind them.

## License

Pun is open-sourced software licensed under the [MIT license](LICENSE). The licenses of ported code are in [licenses/THIRD_PARTY_NOTICES.md](licenses/THIRD_PARTY_NOTICES.md), and those of the bundled Rust crates in [licenses/THIRD_PARTY_LICENSES.md](licenses/THIRD_PARTY_LICENSES.md).
