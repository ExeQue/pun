//! Builds Mago's prelude (the codebase of PHP's built-in stubs) at compile time, as Mago's own binary does, so a run
//! decodes it instead of parsing, scanning and populating every stub: that was most of the start-up time and peak
//! memory of the smaller projects.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let prelude = mago_prelude::Prelude::build();
    let bytes = bincode::serde::encode_to_vec(
        (&prelude.metadata, &prelude.symbol_references),
        bincode::config::standard(),
    )
    .expect("the prelude encodes");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out.join("prelude.bin"), bytes).expect("OUT_DIR is writable");
}
