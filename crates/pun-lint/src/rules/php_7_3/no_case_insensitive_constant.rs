//! Rector 2.8.4's `SensitiveConstantNameRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `PHP_RESERVED_CONSTANTS`.
const PHP_RESERVED_CONSTANTS: &[&str] = &[
    "PHP_VERSION",
    "PHP_MAJOR_VERSION",
    "PHP_MINOR_VERSION",
    "PHP_RELEASE_VERSION",
    "PHP_VERSION_ID",
    "PHP_EXTRA_VERSION",
    "PHP_ZTS",
    "PHP_DEBUG",
    "PHP_MAXPATHLEN",
    "PHP_OS",
    "PHP_OS_FAMILY",
    "PHP_SAPI",
    "PHP_EOL",
    "PHP_INT_MAX",
    "PHP_INT_MIN",
    "PHP_INT_SIZE",
    "PHP_FLOAT_DIG",
    "PHP_FLOAT_EPSILON",
    "PHP_FLOAT_MIN",
    "PHP_FLOAT_MAX",
    "DEFAULT_INCLUDE_PATH",
    "PEAR_INSTALL_DIR",
    "PEAR_EXTENSION_DIR",
    "PHP_EXTENSION_DIR",
    "PHP_PREFIX",
    "PHP_BINDIR",
    "PHP_BINARY",
    "PHP_MANDIR",
    "PHP_LIBDIR",
    "PHP_DATADIR",
    "PHP_SYSCONFDIR",
    "PHP_LOCALSTATEDIR",
    "PHP_CONFIG_FILE_PATH",
    "PHP_CONFIG_FILE_SCAN_DIR",
    "PHP_SHLIB_SUFFIX",
    "PHP_FD_SETSIZE",
    "E_ERROR",
    "E_WARNING",
    "E_PARSE",
    "E_NOTICE",
    "E_CORE_ERROR",
    "E_CORE_WARNING",
    "E_COMPILE_ERROR",
    "E_COMPILE_WARNING",
    "E_USER_ERROR",
    "E_USER_WARNING",
    "E_USER_NOTICE",
    "E_RECOVERABLE_ERROR",
    "E_DEPRECATED",
    "E_USER_DEPRECATED",
    "E_ALL",
    "E_STRICT",
    "__COMPILER_HALT_OFFSET__",
    "TRUE",
    "FALSE",
    "NULL",
];

pub struct NoCaseInsensitiveConstant;

impl Rule for NoCaseInsensitiveConstant {
    fn id(&self) -> &'static str {
        "lint/php-7.3/no-case-insensitive-constant"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ConstantAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ConstantAccess(fetch) = node else {
            return;
        };
        let written = String::from_utf8_lossy(fetch.name.value());
        let name = written.strip_prefix('\\').unwrap_or(&written);
        let uppercased = name.to_ascii_uppercase();
        // is system constant?
        if PHP_RESERVED_CONSTANTS.contains(&uppercased.as_str()) {
            return;
        }
        // is uppercase, all good
        if name == uppercased || uppercased.contains(['\\', '(', '\'']) {
            return;
        }
        // constant is defined in current lower/upper case
        if facts(ctx).codebase.get_constant(name.as_bytes()).is_some() {
            return;
        }
        out.push(Report::fixed(
            node_span(fetch),
            format!(
                "Constants are case-sensitive since PHP 8.0; `{name}` is not defined, write `\\{uppercased}`."
            ),
            Safety::Unsafe,
            vec![Edit::new(node_span(fetch), format!("\\{uppercased}"))],
        ));
    }
}
