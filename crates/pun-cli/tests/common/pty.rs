use std::path::Path;
use std::process::Command;

pub(crate) fn pun(dir: &Path, args: &[&str]) -> (String, String, Option<i32>) {
    pun_with(dir, args, &[])
}

pub(crate) fn pun_with(
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> (String, String, Option<i32>) {
    run(Command::new(super::PUN).args(args), dir, env)
}

fn run(cmd: &mut Command, dir: &Path, env: &[(&str, &str)]) -> (String, String, Option<i32>) {
    let out = super::clean_env(cmd, &["PATH"])
        .current_dir(dir)
        .env("PUN_AGENT", "0")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let s = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (s(out.stdout), s(out.stderr), out.status.code())
}

pub(crate) fn pty(
    dir: &Path,
    args: &str,
    keys: &str,
    env: &[(&str, &str)],
) -> (String, Option<i32>) {
    let pun = format!("{} {args}", super::PUN);
    let script = if cfg!(target_os = "linux") {
        format!("script -qec '{pun}' /dev/null")
    } else {
        format!("script -q /dev/null {pun}")
    };
    let sh = format!("(sleep 1; printf '{keys}'; sleep 2) | {script}");
    let (out, _, code) = run(Command::new("sh").args(["-c", &sh]), dir, env);
    (out.replace('\r', ""), code)
}
