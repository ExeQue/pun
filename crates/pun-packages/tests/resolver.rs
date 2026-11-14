//! Composer's installer fixtures that run `update`, with the lock Composer 2.11.2 wrote for each
//! (`scripts/export-resolver-fixtures.py`): the resolver must lock the same packages, or fail where Composer failed,
//! and the lock it writes must be Composer's text.

use pun_packages::lock::Lock;
use pun_packages::resolver::pool::{ArrayRepository, Repository, Transitive, filtered};
use pun_packages::resolver::{Options, locker, resolve};
use serde_json::{Map, Value};

fn options(args: &[Value]) -> Options {
    let mut o = Options::default();
    for a in args.iter().filter_map(Value::as_str) {
        match a {
            "--with-dependencies" => o.transitive = Transitive::Dependencies,
            "--with-all-dependencies" => o.transitive = Transitive::All,
            "--minimal-changes" => o.minimal_changes = true,
            "--prefer-lowest" => o.prefer_lowest = true,
            "--prefer-stable" => o.prefer_stable = true,
            "--ignore-platform-reqs" => o.ignore_all_platform = true,
            _ if a.starts_with("--ignore-platform-req=") => {
                o.ignore_platform
                    .push(a["--ignore-platform-req=".len()..].to_string());
            }
            _ if a.starts_with('-') => {}
            name => o.allow_list.push(name.to_string()),
        }
    }
    o
}

fn as_map(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn resolves_as_composer() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/resolver.json")).expect("valid fixtures");
    let platform: Vec<Map<String, Value>> = fixtures["platform"]
        .as_array()
        .unwrap()
        .iter()
        .map(as_map)
        .collect();
    let mut failures = vec![];
    let cases = fixtures["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let manifest = as_map(&case["composer"]);
        let repositories: Vec<Box<dyn Repository>> =
            (manifest.get("repositories").and_then(Value::as_array))
                .into_iter()
                .flatten()
                .map(|r| {
                    filtered(
                        Box::new(ArrayRepository::from_config(&as_map(r)).unwrap()),
                        &as_map(r),
                    )
                    .unwrap()
                })
                .collect();
        let lock = case.get("lock").map(|l| {
            let mut l = l.clone();
            l["content-hash"] = "".into();
            Lock::from_composer_lock(&l.to_string()).unwrap()
        });
        let options = options(case["args"].as_array().unwrap());
        let mut text = None;
        let got = match resolve(
            &manifest,
            platform.clone(),
            repositories,
            lock.as_ref(),
            &options,
        ) {
            Err(_) | Ok(Err(_)) => "fails".to_string(),
            Ok(Ok(r)) => {
                let list = |ids: &[usize]| -> Vec<Value> {
                    ids.iter()
                        .map(|&p| {
                            serde_json::json!([r.arena[p].pretty_name, r.arena[p].pretty_version])
                        })
                        .collect()
                };
                text = Some(locker::lock(&r, &manifest, &options).to_composer_lock());
                serde_json::json!({"packages": list(&r.packages), "packages-dev": list(&r.packages_dev)}).to_string()
            }
        };
        let expect = &case["expect"];
        let want = if expect.get("exit").is_some() {
            "fails".to_string()
        } else {
            serde_json::json!({"packages": expect["packages"], "packages-dev": expect["packages-dev"]}).to_string()
        };
        if got != want {
            failures.push(format!("{name}\n  got:  {got}\n  want: {want}"));
        } else if let (Some(text), Some(Value::String(written))) = (text, expect.get("text")) {
            // The content-hash covers the composer.json the export wrote, with Packagist turned off.
            let hash = |t: &str| {
                t.lines()
                    .filter(|l| !l.contains("\"content-hash\""))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            if hash(&text) != hash(written) {
                failures.push(format!(
                    "{name}: the lock text differs\n--- got\n{text}\n--- want\n{written}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
