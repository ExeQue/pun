//! `cargo run -p punphp-blade --example dump -- <mode> <file>`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[2]).expect("read input");
    println!("{}", pun_blade::debug::dump(&args[1], &text));
}
