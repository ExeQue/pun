//! phpstan/phpstan-mockery 2.0.0's PHPStan extension (`extension.neon`), on with `[extensions] mockery = "on"`
//! (`pun_config::extensions::MOCKERY`). Not ported yet.
//!
//! The port adds `pub struct Mockery` here with an `AnalyseExtension` impl, `pub use mockery::Mockery;` in `lib.rs`,
//! and a `mockery: Option<crate::Mockery>` field on `Project`, built in `Project::from_config` and chained in
//! `Project::extensions()`, as `carbon.rs` is.
