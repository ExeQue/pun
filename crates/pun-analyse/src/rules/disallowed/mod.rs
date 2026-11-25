//! spaze/phpstan-disallowed-calls 4.15.1's rules: the `analyse/disallowed/*` rules, which run with
//! `[extensions] disallowed-calls = "on"` (`pun_config::extensions::DISALLOWED_CALLS`, which owns the `disallowed`
//! group). Not ported yet.
//!
//! An entry's `errorIdentifier` goes into `Project::extension_identifiers`, so `Checks::keep_reported` keeps the
//! errors that carry it.
