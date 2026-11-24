use crate::analyse::Finding;
use crate::render::RunError;
use crate::report::Abort;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) const DEFAULT: &str = "pun-baseline.toml";

const HEADER: &str = "# Pun's analyse baseline: errors `pun analyse` doesn't report. Regenerate it with\n# `pun analyse --generate-baseline`.\n\n";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Baseline {
    variant: Variant,
    #[serde(default)]
    issues: Vec<Issue>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Variant {
    Loose,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Issue {
    file: String,
    code: String,
    message: String,
    count: u32,
}

impl Baseline {
    pub(crate) fn of(findings: &[Finding]) -> Baseline {
        let mut counts: BTreeMap<(String, String, String), u32> = BTreeMap::new();
        for f in findings {
            *counts
                .entry((f.rel.replace('\\', "/"), f.rule.clone(), f.message.clone()))
                .or_default() += 1;
        }
        Baseline {
            variant: Variant::Loose,
            issues: (counts.into_iter())
                .map(|((file, code, message), count)| Issue {
                    file,
                    code,
                    message,
                    count,
                })
                .collect(),
        }
    }

    pub(crate) fn read(path: &Path) -> Result<Baseline> {
        let fail = |message: String| -> anyhow::Error {
            let error = RunError {
                path: Some(path.to_path_buf()),
                line: None,
                message: message.clone(),
                hint: Some("run `pun analyse --generate-baseline` to write it again".into()),
            };
            Abort::pun(format!("{}: {message}", path.display()), error).into()
        };
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(format!("can't read the baseline: {e}")))?;
        toml::from_str(&text).map_err(|e| fail(format!("invalid baseline: {}", e.message())))
    }

    pub(crate) fn write(&self, path: &Path) -> Result<()> {
        let text = format!("{HEADER}{}", toml::to_string(self)?);
        crate::run::write_generated(path, text.as_bytes()).map_err(|e| {
            let message = format!("can't write the baseline to {}: {e}", path.display());
            Abort::plain(message, None).into()
        })
    }

    pub(crate) fn filter(&self, findings: &mut Vec<Finding>) -> usize {
        let mut left: BTreeMap<(String, String, String), u32> = (self.issues.iter())
            .map(|i| ((i.file.clone(), i.code.clone(), i.message.clone()), i.count))
            .collect();
        findings.retain(|f| {
            if !f.ignorable() {
                return true;
            }
            let key = (f.rel.replace('\\', "/"), f.rule.clone(), f.message.clone());
            match left.get_mut(&key) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    false
                }
                _ => true,
            }
        });
        left.values().map(|&n| n as usize).sum()
    }
}
