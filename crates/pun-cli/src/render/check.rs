use super::{Renderer, Role, Step, Summary, plural};
use std::fmt::Write as _;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub(crate) struct CheckItem {
    pub(crate) path: PathBuf,
    pub(crate) at: String,
    pub(crate) rules: Vec<String>,
    pub(crate) message: Option<String>,
    pub(crate) below: String,
}

#[derive(Clone, Debug)]
pub(crate) struct StepLine<'a> {
    pub(crate) step: Step,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) fixable: usize,
    pub(crate) files: usize,
    pub(crate) text: Option<String>,
    pub(crate) failed: bool,
    pub(crate) items: &'a [CheckItem],
}

impl Renderer {
    pub(crate) fn check_step(&self, s: &StepLine) -> String {
        let (glyph, role) = if s.errors > 0 || s.failed {
            (self.look.glyph("✖", "x"), Role::Error)
        } else if s.warnings > 0 {
            (self.look.glyph("⚠", "!"), Role::Warning)
        } else {
            (self.look.glyph("✔", "ok"), Role::Ok)
        };
        let dot = self.look.glyph("·", "-");
        let text = s.text.clone().unwrap_or_else(|| {
            let mut counts = vec![];
            if s.errors > 0 {
                counts.push(plural(s.errors, "error", "errors"));
            }
            if s.warnings > 0 {
                counts.push(plural(s.warnings, "warning", "warnings"));
            }
            if counts.is_empty() {
                return "No problems".into();
            }
            let mut text = format!(
                "{} in {}",
                counts.join(", "),
                plural(s.files, "file", "files")
            );
            if s.fixable > 0 {
                let _ = write!(text, " {dot} {} fixable", s.fixable);
            }
            text
        });
        let mut out = format!(
            "{} {}  {text}\n",
            self.look.paint(role, glyph),
            self.look.pill(s.step)
        );
        for item in s.items {
            out.push_str(&self.check_item(item));
        }
        if !s.items.is_empty() && !self.concise {
            out.push('\n');
        }
        out
    }

    fn check_item(&self, item: &CheckItem) -> String {
        let location = format!("{}{}", item.path.display(), item.at);
        let location = self.path_link(&item.path, &location);
        let rules: Vec<String> = item.rules.iter().map(|r| self.rule(r)).collect();
        let rule = rules.join(", ");
        let message = item.message.as_deref().map(super::visible);
        let mut out = match &message {
            Some(m) if self.concise => {
                format!("   {location}  {rule}  {}\n", self.look.paint(Role::Dim, m))
            }
            Some(m) => format!(
                "   {location}  {rule}\n   {}\n",
                self.look.paint(Role::Dim, m)
            ),
            None => format!("   {location}  {rule}\n"),
        };
        out.push_str(&item.below);
        out
    }

    pub(crate) fn check_summary(&self, s: &Summary) -> String {
        let mut parts = vec![];
        if s.errors > 0 {
            parts.push(plural(s.errors, "error", "errors"));
        }
        if s.warnings > 0 {
            parts.push(plural(s.warnings, "warning", "warnings"));
        }
        if s.fixable > 0 {
            parts.push(format!("{} fixable (run {})", s.fixable, s.fix_command));
        }
        if s.need_formatting > 0 {
            parts.push(plural(
                s.need_formatting,
                "file needs formatting",
                "files need formatting",
            ));
        }
        let (glyph, role) = if s.errors > 0 || s.need_formatting > 0 {
            (self.look.glyph("✖", "x"), Role::Error)
        } else if s.warnings > 0 {
            (self.look.glyph("⚠", "!"), Role::Warning)
        } else {
            parts.push("No problems".into());
            (self.look.glyph("✔", "ok"), Role::Ok)
        };
        let dot = self.look.glyph("·", "-");
        format!(
            "{} {}\n",
            self.look.paint(role, glyph),
            self.look
                .paint(Role::Summary, &parts.join(&format!(" {dot} ")))
        )
    }
}
