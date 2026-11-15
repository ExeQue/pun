use std::fmt;
use std::str::FromStr;

/// A PHP version, held as PHP's `PHP_VERSION_ID`: `80400` for 8.4.0, `80511` for 8.5.11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhpVersion(u32);

impl PhpVersion {
    /// The oldest PHP version Pun supports.
    pub const FLOOR: PhpVersion = PhpVersion::new(8, 4);
    /// The newest PHP version Pun knows.
    pub const NEWEST: PhpVersion = PhpVersion::new(8, 5);

    pub const fn new(major: u32, minor: u32) -> PhpVersion {
        PhpVersion(major * 10_000 + minor * 100)
    }

    pub const fn from_id(id: u32) -> PhpVersion {
        PhpVersion(id)
    }

    /// PHP's `PHP_VERSION_ID` for this version.
    pub const fn id(self) -> u32 {
        self.0
    }

    pub const fn major(self) -> u32 {
        self.0 / 10_000
    }

    pub const fn minor(self) -> u32 {
        self.0 / 100 % 100
    }

    pub const fn release(self) -> u32 {
        self.0 % 100
    }

    #[cfg(feature = "mago")]
    pub fn to_mago(self) -> mago_php_version::PHPVersion {
        mago_php_version::PHPVersion::new(self.major(), self.minor(), self.release())
    }
}

/// Reads `major.minor`, such as `8.4`.
impl FromStr for PhpVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<PhpVersion, String> {
        let bad = || format!("expected a PHP version as major.minor, such as \"8.4\"; got \"{s}\"");
        let (major, minor) = s.split_once('.').ok_or_else(bad)?;
        let major: u8 = major.parse().map_err(|_| bad())?;
        let minor: u8 = minor.parse().map_err(|_| bad())?;
        Ok(PhpVersion::new(u32::from(major), u32::from(minor)))
    }
}

/// Writes `major.minor`, such as `8.4`.
impl fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major(), self.minor())
    }
}

#[cfg(test)]
mod tests {
    use super::PhpVersion;

    #[test]
    fn php_version_id_parts() {
        let v = PhpVersion::from_id(80511);
        assert_eq!((v.major(), v.minor(), v.release()), (8, 5, 11));
        assert_eq!(PhpVersion::new(8, 4).id(), 80400);
        assert!(PhpVersion::new(8, 4) < PhpVersion::from_id(80401));
        assert_eq!(v.to_string(), "8.5");
    }

    #[test]
    fn parses_major_minor() {
        assert_eq!("8.5".parse(), Ok(PhpVersion::from_id(80500)));
        assert!("8".parse::<PhpVersion>().is_err());
        assert!("8.x".parse::<PhpVersion>().is_err());
        assert!("x.4".parse::<PhpVersion>().is_err());
    }
}
