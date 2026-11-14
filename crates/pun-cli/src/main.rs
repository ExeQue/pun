use std::process::ExitCode;

/// The Linux release binaries link musl, whose allocator is slow when many threads allocate at once: on
/// laravel-framework `pun check` took 129 s against glibc's 11 s, and resolving Pinkary 10 s against 2 s. With
/// mimalloc both match glibc.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    punphp::main()
}
