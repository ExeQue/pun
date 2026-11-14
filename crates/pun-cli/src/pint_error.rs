use crate::console::{self, escape};
use std::fmt::Write as _;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Thrown {
    NotReadable,
    DirectoryNotFound,
    ReadFailed,
    NotWritable,
    NotEnabled,
    Analysing,
    UndefinedArrayKey,
    ReportNotWritable,
    JsonMalformed,
    Console,
}

#[derive(Debug)]
pub(crate) enum PintError {
    Read(PathBuf),
    Write(PathBuf),
}

impl std::fmt::Display for PintError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PintError::Read(p) => write!(f, "Failed to read content from \"{}\".", p.display()),
            PintError::Write(p) => write!(
                f,
                "Cannot write to file \"{}\" as it is not writable.",
                p.display()
            ),
        }
    }
}

impl std::error::Error for PintError {}

impl Thrown {
    pub(crate) fn of(e: &anyhow::Error) -> Thrown {
        use pun_format::AnnotationError as A;
        use pun_workspace::FinderError as F;
        if let Some(f) = e.downcast_ref::<F>() {
            return match f {
                F::NotReadable(_) => Thrown::NotReadable,
                F::DirectoryNotFound(_) => Thrown::DirectoryNotFound,
            };
        }
        if let Some(a) = e.downcast_ref::<A>() {
            return match a {
                A::NotEnabled(_) => Thrown::NotEnabled,
                A::Analysing { .. } => Thrown::Analysing,
                A::UndefinedArrayKey => Thrown::UndefinedArrayKey,
            };
        }
        match e.downcast_ref::<PintError>() {
            Some(PintError::Read(_)) => Thrown::ReadFailed,
            Some(PintError::Write(_)) => Thrown::NotWritable,
            None => Thrown::Console,
        }
    }
}

pub(crate) fn render(message: &str, thrown: Thrown) -> String {
    let args: Vec<String> = std::env::args().skip(1).take_while(|a| a != "--").collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let decorated = console::decorated(1, has("--ansi"), has("--no-ansi"));
    let opt = |long: &str, short: &str| {
        args.iter()
            .any(|a| a == long || a.starts_with(&format!("{long}=")) || a.starts_with(short))
    };
    let verbose = !opt("--quiet", "-q") && opt("--verbose", "-v");
    let mut out = String::from("\n");
    let nested = (thrown == Thrown::Analysing)
        .then(|| message.split_once("\": ").map(|(_, inner)| inner))
        .flatten();
    for (i, m) in std::iter::once(message).chain(nested).enumerate() {
        let (file, line, class, console_error) = origin(thrown, i > 0);
        if verbose || !console_error {
            let name = file.rsplit('/').next().unwrap_or(file);
            out += &console::format(
                &format!("<comment>In {name} line {line}:</comment>\n"),
                decorated,
            );
        }
        out += &abort_box(
            verbose.then(|| format!("  [{class}]  ")).as_deref(),
            m,
            decorated,
        );
        if verbose {
            out += &console::format(
                &format!(
                    "<comment>Exception trace:</comment>\n  at <info>phar://pint/{file}:{line}</info>\n\n"
                ),
                decorated,
            );
        }
    }
    out
}

fn origin(thrown: Thrown, nested: bool) -> (&'static str, u32, &'static str, bool) {
    let at = |file: &'static str, line, class| (file, line, class, false);
    match thrown {
        _ if nested => at(
            "vendor/friendsofphp/php-cs-fixer/src/Tokenizer/Analyzer/FixerAnnotationAnalyzer.php",
            115,
            "RuntimeException",
        ),
        Thrown::NotReadable => at(
            "vendor/friendsofphp/php-cs-fixer/src/Console/ConfigurationResolver.php",
            409,
            "PhpCsFixer\\ConfigurationException\\InvalidConfigurationException (16)",
        ),
        Thrown::DirectoryNotFound => at(
            "vendor/symfony/finder/Finder.php",
            648,
            "Symfony\\Component\\Finder\\Exception\\DirectoryNotFoundException",
        ),
        Thrown::ReadFailed => at(
            "vendor/friendsofphp/php-cs-fixer/src/FileReader.php",
            63,
            "RuntimeException",
        ),
        Thrown::NotWritable => at(
            "vendor/friendsofphp/php-cs-fixer/src/Runner/Runner.php",
            736,
            "Symfony\\Component\\Filesystem\\Exception\\IOException",
        ),
        Thrown::NotEnabled => at(
            "vendor/friendsofphp/php-cs-fixer/src/Runner/Runner.php",
            271,
            "RuntimeException",
        ),
        Thrown::Analysing => at(
            "vendor/friendsofphp/php-cs-fixer/src/Runner/Runner.php",
            602,
            "RuntimeException",
        ),
        Thrown::UndefinedArrayKey => at(
            "vendor/friendsofphp/php-cs-fixer/src/Tokenizer/Analyzer/FixerAnnotationAnalyzer.php",
            103,
            "ErrorException",
        ),
        Thrown::ReportNotWritable => at("app/Actions/ElaborateSummary.php", 106, "ErrorException"),
        Thrown::JsonMalformed => at(
            "vendor/friendsofphp/php-cs-fixer/src/Console/Report/FixReport/JsonReporter.php",
            63,
            "JsonException (5)",
        ),
        Thrown::Console => (
            "vendor/laravel-zero/framework/src/Application.php",
            154,
            "LaravelZero\\Framework\\Exceptions\\ConsoleException",
            true,
        ),
    }
}

fn abort_box(title: Option<&str>, message: &str, decorated: bool) -> String {
    let width = match console::width() {
        0 => usize::MAX,
        w => w - 1,
    };
    let chunk = width.saturating_sub(4).max(1);
    let mut lines: Vec<String> = vec![];
    for line in message
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
    {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= chunk {
            lines.push(line.to_string());
        } else {
            lines.extend(
                chars
                    .chunks(chunk)
                    .map(|c| format!("{:chunk$}", c.iter().collect::<String>())),
            );
        }
    }
    let title_len = title.map_or(0, |t| t.chars().count());
    let len = lines
        .iter()
        .map(|l| l.chars().count() + 4)
        .max()
        .unwrap_or(0)
        .max(title_len);
    let empty = format!("<error>{}</error>", " ".repeat(len));
    let mut o = format!("{empty}\n");
    if let Some(t) = title {
        let _ = writeln!(o, "<error>{t}{}</error>", " ".repeat(len - title_len));
    }
    for l in &lines {
        let _ = writeln!(
            o,
            "<error>  {}  {}</error>",
            escape(l),
            " ".repeat(len - l.chars().count() - 4)
        );
    }
    let _ = write!(o, "{empty}\n\n");
    console::format(&o, decorated)
}
