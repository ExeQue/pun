use crate::report::Abort;
use std::path::Path;

pub(crate) fn ensure(root: &Path, enabled: bool) -> Option<Abort> {
    if !enabled {
        return None;
    }
    const REQUIRED: [(&str, &str); 3] = [
        ("prettier", "^3.9.6"),
        ("prettier-plugin-blade", "^3.3.3"),
        ("prettier-plugin-tailwindcss", "^0.8.1"),
    ];
    let probes = pun_fixers::prettier::probe(&REQUIRED.map(|r| r.0));
    let Some(probes) = probes else {
        let mut abort = Abort::pint(
            "The rules enabled in your pint configuration require a JavaScript runtime (Node.js or Bun) to be installed.",
            Some("install Node.js or Bun, or turn `format/laravel/blade` off in pun.toml".into()),
        );
        abort.error.message =
            "`format/laravel/blade` needs a JavaScript runtime (Node.js or Bun), and there is none on PATH".into();
        return Some(abort);
    };
    let manager = if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        ("bun", "add -d")
    } else if root.join("pnpm-lock.yaml").exists() {
        ("pnpm", "add -D")
    } else if root.join("yarn.lock").exists() {
        ("yarn", "add -D")
    } else {
        ("npm", "install -D")
    };
    let mut missing = vec![];
    let mut outdated = vec![];
    for ((pkg, constraint), probe) in REQUIRED.into_iter().zip(probes) {
        match probe {
            None => missing.push(pkg),
            Some(v) if !v.is_empty() && !caret_satisfies(&v, &constraint[1..]) => {
                outdated.push((pkg, v, constraint));
            }
            Some(_) => {}
        }
    }
    if !missing.is_empty() || !outdated.is_empty() {
        pun_fixers::prettier::stop();
    }
    if !missing.is_empty() {
        let warning = if crate::agent::enabled() {
            String::new()
        } else {
            format!(
                "\n\x1b[33m The rules enabled in your pint configuration require the following prettier dependencies to be installed using [{}]: {}.\x1b[39m\n\n",
                manager.0,
                missing.join(", ")
            )
        };
        let msg = format!(
            "The rules enabled in your pint configuration require the following prettier dependencies to be installed: {}.",
            missing.join(", ")
        );
        let install = format!(
            "install them: {} {} {}",
            manager.0,
            manager.1,
            missing.join(" ")
        );
        let mut abort = Abort::pint(msg, Some(install));
        abort.before = warning;
        abort.error.message = format!(
            "`format/laravel/blade` needs {} in the project",
            missing.join(", ")
        );
        return Some(abort);
    }
    if !outdated.is_empty() {
        let list: Vec<String> = outdated
            .iter()
            .map(|(p, v, c)| format!("  - {p} (installed: {v}, required: {c})"))
            .collect();
        let specs: Vec<String> = outdated
            .iter()
            .map(|(p, _, c)| format!("{p}@{c}"))
            .collect();
        let msg = format!(
            "The following prettier dependencies do not satisfy the versions required by your pint configuration:\n{}\n\nUpdate them using [{}]: {} {} {}",
            list.join("\n"),
            manager.0,
            manager.0,
            manager.1,
            specs.join(" ")
        );
        let have: Vec<String> = (outdated.iter())
            .map(|(p, v, c)| format!("{p} {v} (needs {c})"))
            .collect();
        let mut abort = Abort::pint(
            msg,
            Some(format!(
                "update them: {} {} {}",
                manager.0,
                manager.1,
                specs.join(" ")
            )),
        );
        abort.error.message = format!(
            "`format/laravel/blade` needs newer prettier packages: {}",
            have.join(", ")
        );
        return Some(abort);
    }
    None
}

fn caret_satisfies(version: &str, base: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        let mut parts: Vec<u64> = (s.trim_start_matches('v').split(['.', '-', '+']))
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        parts.resize(3, 0);
        parts
    };
    let (v, b) = (parse(version), parse(base));
    let first = b.iter().position(|&n| n != 0).unwrap_or(2);
    v >= b && v[..=first] == b[..=first]
}

#[cfg(test)]
mod tests {
    #[test]
    fn caret_ranges() {
        use super::caret_satisfies as ok;
        assert!(ok("0.8.3", "0.8.0"));
        assert!(!ok("0.9.0", "0.8.0"));
        assert!(!ok("1", "0.8.0"));
        assert!(ok("3.4", "3.0.0"));
        assert!(!ok("4", "3.0.0"));
        assert!(!ok("2.9.9", "3.0.0"));
    }
}
