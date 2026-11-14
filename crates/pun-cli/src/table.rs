const MAX_LINE_LENGTH: usize = 120;

pub(crate) struct Style {
    width: usize,
}

pub(crate) fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    let c = c as u32;
    let wide = matches!(c,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x1FA70..=0x1FAFF
        | 0x20000..=0x2FFFD
        | 0x30000..=0x3FFFD);
    if wide { 2 } else { 1 }
}

fn wrap(line: &str, max: usize) -> Vec<String> {
    if max == 0 || width(line) <= max {
        return vec![line.to_string()];
    }
    let mut out = vec![];
    let mut current = String::new();
    for word in line.split(' ') {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if width(&candidate) <= max {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        let mut piece = String::new();
        for c in word.chars() {
            if width(&piece) + char_width(c) > max {
                out.push(std::mem::take(&mut piece));
            }
            piece.push(c);
        }
        current = piece;
    }
    out.push(current);
    out
}

fn pad(text: &str, columns: usize) -> String {
    format!("{text}{}", " ".repeat(columns.saturating_sub(width(text))))
}

impl Style {
    pub(crate) fn new(width: usize) -> Style {
        Style { width }
    }

    pub(crate) fn table(&self, header: [&str; 2], rows: &[[String; 2]]) -> String {
        let terminal = self.width.saturating_sub(2);
        let first = rows
            .iter()
            .map(|r| width(&r[0]))
            .fold(header[0].len(), usize::max);
        let max = terminal.saturating_sub(first + 5);
        let wrapped = |a: &str, b: &str| -> Vec<[String; 2]> {
            let lines = b.split('\n').flat_map(|l| wrap(l, max));
            (lines.enumerate())
                .map(|(i, line)| [if i == 0 { a } else { "" }.to_string(), line])
                .collect()
        };
        let mut lines = wrapped(header[0], header[1]);
        let separator = lines.len();
        for [a, b] in rows {
            lines.extend(wrapped(a, b));
        }
        let widths = [0, 1].map(|c| {
            let w = lines.iter().map(|l| width(&l[c])).max().unwrap_or(0);
            if c == 1 { w.min(max.max(1)) } else { w }
        });
        let border = format!(
            " {} {} \n",
            "-".repeat(widths[0] + 2),
            "-".repeat(widths[1] + 2)
        );
        let mut out = border.clone();
        for (i, [a, b]) in lines.iter().enumerate() {
            if i == separator {
                out.push_str(&border);
            }
            out.push_str(&format!(
                "  {}   {}  \n",
                pad(a, widths[0]),
                pad(b, widths[1])
            ));
        }
        out.push_str(&border);
        out.push('\n');
        out
    }

    pub(crate) fn block(&self, kind: &str, message: &str, tables: usize) -> String {
        let line = format!(" [{kind}] {message}");
        let before = if tables >= 2 { "" } else { "\n" };
        format!(
            "{before}{}\n\n",
            pad(&line, self.width.min(MAX_LINE_LENGTH))
        )
    }
}
