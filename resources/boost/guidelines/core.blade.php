@php
/** @var \Laravel\Boost\Install\GuidelineAssist $assist */
@endphp
# Pun

Pun formats, lints, analyses and installs this project's PHP code. Its configuration is `pun.toml`. The dependencies stay in `composer.json` and `composer.lock`, as in any Composer project.

- After you change PHP files, run `{{ $assist->binCommand('pun') }} check --fix --dirty`. It formats, applies lint fixes and reports type errors in one run. Fix every error it reports before you finish.
- Use Pun in place of Pint, PHP-CS-Fixer, Rector and PHPStan. Do not run those tools in this project.
- Use `{{ $assist->binCommand('pun') }} require`, `remove`, `install` and `update` for Composer packages. They read `composer.json` and write `composer.lock` as Composer does.
- To learn what a rule checks, run `{{ $assist->binCommand('pun') }} explain <rule>`.
