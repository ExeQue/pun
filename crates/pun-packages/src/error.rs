use std::fmt::{self, Display};

#[derive(Debug)]
pub enum Error {
    Message(String),
    Context { message: String, source: Box<Error> },
    Io(std::io::Error),
    Json(serde_json::Error),
    Toml(toml::de::Error),
    Pcre(pcre2::Error),
    Http(ureq::Error),
    Zip(zip::result::ZipError),
    ThreadPool(rayon::ThreadPoolBuildError),
    ParseInt(std::num::ParseIntError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn msg(message: impl Display) -> Error {
        Error::Message(message.to_string())
    }

    /// Another crate's error with each of its causes, so `{:#}` still prints the whole chain.
    pub fn foreign(e: &(dyn std::error::Error + 'static)) -> Error {
        let message = e.to_string();
        match e.source() {
            Some(cause) => Error::Context {
                message,
                source: Box::new(Error::foreign(cause)),
            },
            None => Error::Message(message),
        }
    }

    fn inner(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Message(_) | Error::Context { .. } => None,
            Error::Io(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Toml(e) => Some(e),
            Error::Pcre(e) => Some(e),
            Error::Http(e) => Some(e),
            Error::Zip(e) => Some(e),
            Error::ThreadPool(e) => Some(e),
            Error::ParseInt(e) => Some(e),
        }
    }
}

impl Display for Error {
    /// `{:#}` appends each cause after `: `, as anyhow does.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Error::Message(m) | Error::Context { message: m, .. } = self {
            f.write_str(m)?;
        } else if let Some(e) = self.inner() {
            write!(f, "{e}")?;
        }
        if f.alternate() {
            let mut cause = std::error::Error::source(self);
            while let Some(e) = cause {
                write!(f, ": {e}")?;
                cause = e.source();
            }
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Context { source, .. } => Some(source.as_ref()),
            e => e.inner().and_then(std::error::Error::source),
        }
    }
}

macro_rules! from {
    ($($variant:ident($type:ty)),* $(,)?) => {
        $(impl From<$type> for Error {
            fn from(e: $type) -> Error {
                Error::$variant(e)
            }
        })*
    };
}

from!(
    Io(std::io::Error),
    Json(serde_json::Error),
    Toml(toml::de::Error),
    Pcre(pcre2::Error),
    Http(ureq::Error),
    Zip(zip::result::ZipError),
    ThreadPool(rayon::ThreadPoolBuildError),
    ParseInt(std::num::ParseIntError),
);

pub trait Context<T> {
    fn context(self, message: impl Display) -> Result<T>;
    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T>;
}

impl<T, E: Into<Error>> Context<T> for std::result::Result<T, E> {
    fn context(self, message: impl Display) -> Result<T> {
        self.with_context(|| message)
    }

    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T> {
        self.map_err(|e| Error::Context {
            message: message().to_string(),
            source: Box::new(e.into()),
        })
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, message: impl Display) -> Result<T> {
        self.ok_or_else(|| Error::msg(message))
    }

    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T> {
        self.ok_or_else(|| Error::msg(message()))
    }
}

macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::Message(format!($($arg)*)))
    };
}
pub(crate) use bail;
