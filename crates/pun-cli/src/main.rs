use std::process::ExitCode;

/// mimalloc on every target. musl's allocator is slow when many threads allocate at once: on laravel-framework
/// `pun check` took 129 s against glibc's 11 s, and resolving Pinkary 10 s against 2 s. Against glibc, mimalloc makes
/// laravel-framework `pun analyse` 8% faster (38.1 s to 35.1 s) and lint 5% faster.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    punphp::main()
}
