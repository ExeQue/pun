//! `cargo run -p punphp-blade --example dump -- <mode> <file>`

#[expect(clippy::print_stdout, reason = "a debug tool that prints its result")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[2]).expect("read input");
    if args[1] == "fmt" {
        print!("{}", pun_blade::format(&text));
    } else {
        println!("{}", pun_blade::debug::dump(&args[1], &text));
    }
}
