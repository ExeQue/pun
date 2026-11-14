use crate::error::{Context, Result, bail};
use crate::version::normalize;
use pcre2::bytes::Regex;
use serde_json::{Map, Value, json};
use std::process::Command;
use std::sync::LazyLock;

pub const COMPOSER: &str = "2.11.2";
pub const PLUGIN_API: &str = "2.11.0";
pub const RUNTIME_API: &str = "2.2.2";

const PROBE: &str = r#"
$out = ['php' => PHP_VERSION, 'debug' => (bool) PHP_DEBUG, 'zts' => defined('PHP_ZTS') && PHP_ZTS,
    'bits64' => PHP_INT_SIZE === 8, 'ipv6' => defined('AF_INET6') || @inet_pton('::') !== false, 'ext' => [], 'lib' => []];
$loaded = get_loaded_extensions();
foreach ($loaded as $name) {
    if ($name === 'standard' || $name === 'Core') continue;
    $out['ext'][] = [$name, phpversion($name) ?: '0'];
}
$lib = function ($name, $version, $replaces = [], $provides = []) use (&$out) {
    if ($version !== null) $out['lib'][] = [$name, (string) $version, array_values($replaces), array_values($provides)];
};
$c = fn ($n) => defined($n) ? constant($n) : null;
$info = function ($e) { ob_start(); (new ReflectionExtension($e))->info(); return (string) ob_get_clean(); };
$m = fn ($re, $s) => preg_match($re, $s, $g) ? $g : null;
$alpha = fn ($a) => strlen($a) * (-ord('a') + 1) + array_sum(array_map('ord', str_split($a)));
$id = fn ($v) => sprintf('%d.%d.%d', $v / 10000, (int) ($v / 100) % 100, $v % 100);
$openssl = function ($v, &$fips) use ($alpha) {
    $fips = false;
    if (!preg_match('/^(?<version>[0-9.]+)(?<patch>[a-z]{0,2})(?<suffix>(?:-?(?:dev|pre|alpha|beta|rc|fips)[\d]*)*)(?:-\w+)?(?: \(.+?\))?$/', $v, $g)) return null;
    $patch = version_compare($g['version'], '3.0.0', '<') ? '.'.$alpha($g['patch']) : '';
    $fips = strpos($g['suffix'], 'fips') !== false;
    return rtrim($g['version'].$patch.strtr('-'.ltrim($g['suffix'], '-'), ['-fips' => '', '-pre' => '-alpha']), '-');
};
foreach ($loaded as $name) {
    switch ($name) {
        case 'amqp':
            $i = $info($name);
            if ($g = $m('/^librabbitmq version => (?<v>.+)$/im', $i)) $lib($name.'-librabbitmq', $g['v']);
            if ($g = $m('/^AMQP protocol version => (?<v>.+)$/im', $i)) $lib($name.'-protocol', str_replace('-', '.', $g['v']));
            break;
        case 'bz2':
            if ($g = $m('/^BZip2 Version => (?<v>.*),/im', $info($name))) $lib($name, $g['v']);
            break;
        case 'curl':
            $lib($name, curl_version()['version']);
            $i = $info($name);
            if ($g = $m('{^SSL Version => (?<l>[^\r\n/]+)/(?<v>[^\r\n]+?)\r?$}im', $i)) {
                $l = strtolower($g['l']);
                if ($l === 'openssl') {
                    $v = $openssl($g['v'], $fips);
                    $lib($name.'-openssl'.($fips ? '-fips' : ''), $v, [], $fips ? ['curl-openssl'] : []);
                } else {
                    if (str_starts_with($l, '(securetransport)') && ($s = $m('{^\(securetransport\) ([a-z0-9]+)}', $l))) {
                        [$short, $ssl] = ['securetransport', 'curl-'.$s[1]];
                    } else {
                        [$short, $ssl] = [$l, 'curl-openssl'];
                    }
                    $lib($name.'-'.$short, $g['v'], [$ssl]);
                }
            }
            if ($g = $m('{^libSSH Version => (?<l>[^\r\n/]+)/(?<v>.+?)(?:/.*)?$}im', $i)) $lib($name.'-'.strtolower($g['l']), $g['v']);
            if ($g = $m('{^ZLib Version => (?<v>.+)$}im', $i)) $lib($name.'-zlib', $g['v']);
            break;
        case 'date':
            $i = $info($name);
            if ($g = $m('/^timelib version => (?<v>.+)$/im', $i)) $lib($name.'-timelib', $g['v']);
            if (($s = $m('/^Timezone Database => (?<s>internal|external)$/im', $i))
                && ($g = $m('/^"Olson" Timezone Database Version => (?<v>.+?)(?:\.system)?$/im', $i))) {
                if ($s['s'] === 'external' && in_array('timezonedb', $loaded, true)) $lib('timezonedb-zoneinfo', $g['v'], [$name.'-zoneinfo']);
                else $lib($name.'-zoneinfo', $g['v']);
            }
            break;
        case 'fileinfo':
            if ($g = $m('/^libmagic => (?<v>.+)$/im', $info($name))) $lib($name.'-libmagic', $g['v']);
            break;
        case 'gd':
            $lib($name, $c('GD_VERSION'));
            $i = $info($name);
            if (($g = $m('/^libJPEG Version => (?<v>.+?)(?: compatible)?$/im', $i)) && ($j = $m('/^(?<a>\d+)(?<b>[a-z]*)$/', $g['v']))) $lib($name.'-libjpeg', $j['a'].'.'.$alpha($j['b']));
            if ($g = $m('/^libPNG Version => (?<v>.+)$/im', $i)) $lib($name.'-libpng', $g['v']);
            if ($g = $m('/^FreeType Version => (?<v>.+)$/im', $i)) $lib($name.'-freetype', $g['v']);
            if ($g = $m('/^libXpm Version => (?<v>\d+)$/im', $i)) $lib($name.'-libxpm', $id((int) $g['v']));
            break;
        case 'gmp': $lib($name, $c('GMP_VERSION')); break;
        case 'iconv': $lib($name, $c('ICONV_VERSION')); break;
        case 'intl':
            $i = $info($name);
            if (defined('INTL_ICU_VERSION')) $lib('icu', INTL_ICU_VERSION);
            elseif ($g = $m('/^ICU version => (?<v>.+)$/im', $i)) $lib('icu', $g['v']);
            if (($g = $m('/^ICU TZData version => (?<v>.*)$/im', $i)) && ($z = $m('/^(?<y>\d{4})(?<r>[a-z]*)$/', $g['v']))) $lib('icu-zoneinfo', $z['y'].'.'.$alpha($z['r']));
            if (class_exists('ResourceBundle') && ($b = ResourceBundle::create('root', 'ICUDATA', false)) !== null) $lib('icu-cldr', $b->get('Version'));
            if (class_exists('IntlChar')) $lib('icu-unicode', implode('.', array_slice(IntlChar::getUnicodeVersion(), 0, 3)));
            break;
        case 'imagick':
            $v = (new Imagick())->getVersion();
            if ($g = $m('/^ImageMagick (?<v>[\d.]+)(?:-(?<p>\d+))?/', $v['versionString'])) $lib($name.'-imagemagick', $g['v'].(isset($g['p']) ? '.'.$g['p'] : ''), ['imagick']);
            break;
        case 'ldap':
            $i = $info($name);
            if (($g = $m('/^Vendor Version => (?<v>\d+)$/im', $i)) && ($n = $m('/^Vendor Name => (?<n>.+)$/im', $i))) $lib($name.'-'.strtolower($n['n']), $id((int) $g['v']));
            break;
        case 'libxml':
            $lib($name, $c('LIBXML_DOTTED_VERSION'), [], array_map(fn ($e) => $e.'-libxml', array_intersect($loaded, ['dom', 'simplexml', 'xml', 'xmlreader', 'xmlwriter'])));
            break;
        case 'mbstring':
            $i = $info($name);
            if ($g = $m('/^libmbfl version => (?<v>.+)$/im', $i)) $lib($name.'-libmbfl', $g['v']);
            if (PHP_VERSION_ID < 80600 && defined('MB_ONIGURUMA_VERSION')) $lib($name.'-oniguruma', MB_ONIGURUMA_VERSION);
            elseif ($g = $m('/^(?:oniguruma|Multibyte regex \(oniguruma\)) version => (?<v>.+)$/im', $i)) $lib($name.'-oniguruma', $g['v']);
            break;
        case 'memcached':
            if ($g = $m('/^libmemcached version => (?<v>.+)$/im', $info($name))) $lib($name.'-libmemcached', $g['v']);
            break;
        case 'openssl':
            if ($g = $m('{^(?:OpenSSL|LibreSSL)?\s*(?<v>\S+)}i', OPENSSL_VERSION_TEXT)) {
                $v = $openssl($g['v'], $fips);
                $lib($name.($fips ? '-fips' : ''), $v, [], $fips ? [$name] : []);
            }
            break;
        case 'pcre':
            $lib($name, preg_replace('{^(\S+).*}', '$1', PCRE_VERSION));
            if ($g = $m('/^PCRE Unicode Version => (?<v>.+)$/im', $info($name))) $lib($name.'-unicode', $g['v']);
            break;
        case 'mysqlnd': case 'pdo_mysql':
            if ($g = $m('/^(?:Client API version|Version) => mysqlnd (?<v>.+?) /mi', $info($name))) $lib($name.'-mysqlnd', $g['v']);
            break;
        case 'mongodb':
            $i = $info($name);
            if ($g = $m('/^libmongoc bundled version => (?<v>.+)$/im', $i)) $lib($name.'-libmongoc', $g['v']);
            if ($g = $m('/^libbson bundled version => (?<v>.+)$/im', $i)) $lib($name.'-libbson', $g['v']);
            break;
        case 'pgsql':
            if (defined('PGSQL_LIBPQ_VERSION')) { $lib('pgsql-libpq', PGSQL_LIBPQ_VERSION); break; }
        case 'pdo_pgsql':
            if ($g = $m('/^PostgreSQL\(libpq\) Version => (?<v>.*)$/im', $info($name))) $lib($name.'-libpq', $g['v']);
            break;
        case 'pq':
            if ($g = $m('/^libpq => (?<c>.+) => (?<v>.+)$/im', $info($name))) $lib($name.'-libpq', $g['v']);
            break;
        case 'rdkafka':
            if (defined('RD_KAFKA_VERSION')) { $k = RD_KAFKA_VERSION; $lib($name.'-librdkafka', sprintf('%d.%d.%d', ($k & 0x7F000000) >> 24, ($k & 0x00FF0000) >> 16, ($k & 0x0000FF00) >> 8)); }
            break;
        case 'libsodium': case 'sodium': $lib('libsodium', $c('SODIUM_LIBRARY_VERSION')); break;
        case 'sqlite3': case 'pdo_sqlite':
            if ($g = $m('/^SQLite Library => (?<v>.+)$/im', $info($name))) $lib($name.'-sqlite', $g['v']);
            break;
        case 'ssh2':
            if ($g = $m('/^libssh2 version => (?<v>.+)$/im', $info($name))) $lib($name.'-libssh2', $g['v']);
            break;
        case 'xsl':
            $lib('libxslt', $c('LIBXSLT_DOTTED_VERSION'), ['xsl']);
            if ($g = $m('/^libxslt compiled against libxml Version => (?<v>.+)$/im', $info('xsl'))) $lib('libxslt-libxml', $g['v']);
            break;
        case 'yaml':
            if ($g = $m('/^LibYAML Version => (?<v>.+)$/im', $info('yaml'))) $lib($name.'-libyaml', $g['v']);
            break;
        case 'zip':
            if (defined('ZipArchive::LIBZIP_VERSION')) $lib($name.'-libzip', ZipArchive::LIBZIP_VERSION, ['zip']);
            break;
        case 'zlib':
            if (defined('ZLIB_VERSION')) $lib($name, ZLIB_VERSION);
            elseif ($g = $m('/^Linked Version => (?<v>.+)$/im', $info($name))) $lib($name, $g['v']);
            break;
    }
}
echo json_encode($out);
"#;

static LEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+\.\d+\.\d+(?:\.\d+)?)").expect("valid pattern"));
static PHP_BASE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^~+-]+)").expect("valid pattern"));

fn first(re: &Regex, s: &str) -> Option<String> {
    let c = re.captures(s.as_bytes()).ok()??;
    Some(String::from_utf8_lossy(c.get(1)?.as_bytes()).into_owned())
}

fn package(name: &str, version: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("name".into(), name.into());
    m.insert("version".into(), version.into());
    m
}

pub fn detect(
    php: &str,
    overrides: Option<&Map<String, Value>>,
) -> Result<Vec<Map<String, Value>>> {
    let output = Command::new(php)
        .args(["-d", "display_errors=stderr", "-r", PROBE])
        .output()
        .with_context(|| format!("can't run {php} to read the platform"))?;
    if !output.status.success() {
        bail!(
            "{php} failed to report the platform: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let probe: Value = serde_json::from_slice(&output.stdout)
        .context("the platform probe printed invalid JSON")?;
    from_probe(&probe, overrides)
}

pub fn from_probe(
    probe: &Value,
    overrides: Option<&Map<String, Value>>,
) -> Result<Vec<Map<String, Value>>> {
    let mut detected = vec![
        package("composer", COMPOSER),
        package("composer-plugin-api", PLUGIN_API),
        package("composer-runtime-api", RUNTIME_API),
    ];
    let php_version = probe["php"].as_str().unwrap_or("0");
    let php_version = match normalize(php_version) {
        Ok(_) => php_version.to_string(),
        Err(_) => first(&PHP_BASE, php_version).unwrap_or_else(|| "0".into()),
    };
    detected.push(package("php", &php_version));
    for (flag, name) in [
        ("debug", "php-debug"),
        ("zts", "php-zts"),
        ("bits64", "php-64bit"),
        ("ipv6", "php-ipv6"),
    ] {
        if probe[flag] == Value::Bool(true) {
            detected.push(package(name, &php_version));
        }
    }
    for ext in probe["ext"].as_array().into_iter().flatten() {
        let (name, version) = (
            ext[0].as_str().unwrap_or_default(),
            ext[1].as_str().unwrap_or("0"),
        );
        let version = match normalize(version) {
            Ok(_) => version.to_string(),
            Err(_) => first(&LEADING, version).unwrap_or_else(|| "0".into()),
        };
        let mut p = package(
            &format!("ext-{}", name.to_ascii_lowercase().replace(' ', "-")),
            &version,
        );
        p.insert("type".into(), "php-ext".into());
        if name == "uuid" {
            p.insert("replace".into(), json!({"lib-uuid": "self.version"}));
        }
        detected.push(p);
    }
    let mut seen = vec![];
    for lib in probe["lib"].as_array().into_iter().flatten() {
        let (name, version) = (
            lib[0].as_str().unwrap_or_default(),
            lib[1].as_str().unwrap_or_default(),
        );
        let name = format!("lib-{name}");
        if normalize(version).is_err()
            || !crate::version::is_platform(&name)
            || seen.contains(&name)
        {
            continue;
        }
        seen.push(name.clone());
        let mut p = package(&name, version);
        for (key, i) in [("replace", 2), ("provide", 3)] {
            let links: Map<String, Value> = (lib[i].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .map(|t| {
                    (
                        format!("lib-{}", t.to_ascii_lowercase()),
                        Value::from("self.version"),
                    )
                })
                .collect();
            if !links.is_empty() {
                p.insert(key.into(), Value::Object(links));
            }
        }
        detected.push(p);
    }
    let overrides = overrides.cloned().unwrap_or_default();
    let mut out = vec![];
    for (name, version) in &overrides {
        let name = name.to_ascii_lowercase();
        if !crate::version::is_platform(&name) {
            bail!("Invalid platform package name in config.platform: {name}");
        }
        if let Some(v) = version.as_str() {
            normalize(v).with_context(|| format!("config.platform.{name}"))?;
            out.push(overridden(&name, v));
        }
    }
    let php_override = overrides.get("php").and_then(Value::as_str);
    for p in detected {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        if overrides.keys().any(|k| k.eq_ignore_ascii_case(&name)) {
            continue;
        }
        if let Some(v) = php_override
            && name.starts_with("php-")
        {
            out.push(overridden(&name, v));
            continue;
        }
        out.push(p);
    }
    Ok(out)
}

fn overridden(name: &str, version: &str) -> Map<String, Value> {
    let mut p = package(name, version);
    p.insert("extra".into(), json!({"config.platform": true}));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[Map<String, Value>]) -> Vec<String> {
        list.iter()
            .map(|p| {
                format!(
                    "{} {}",
                    p["name"].as_str().unwrap(),
                    p["version"].as_str().unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn applies_overrides_as_composer() {
        let probe = json!({"php": "8.5.12~ubuntu", "bits64": true, "ipv6": false, "debug": false, "zts": false,
            "ext": [["Zend OPcache", "8.5.12"], ["mongodb", "2.1.1beta"], ["odd", "1.2.3-weird7"], ["uuid", "1.0"]],
            "lib": [["openssl", "3.0.13", [], []], ["libxml", "2.9.14", [], ["dom"]], ["libxml", "2.9.14", [], []], ["bad", "x", [], []],
                ["zip-libzip", "1.10.1", ["zip"], []], ["bad name", "1.0", [], []]]});
        let got = from_probe(&probe, None).unwrap();
        assert_eq!(
            names(&got),
            [
                "composer 2.11.2",
                "composer-plugin-api 2.11.0",
                "composer-runtime-api 2.2.2",
                "php 8.5.12",
                "php-64bit 8.5.12",
                "ext-zend-opcache 8.5.12",
                "ext-mongodb 2.1.1beta",
                "ext-odd 1.2.3",
                "ext-uuid 1.0",
                "lib-openssl 3.0.13",
                "lib-libxml 2.9.14",
                "lib-zip-libzip 1.10.1",
            ]
        );
        assert_eq!(got[10]["provide"], json!({"lib-dom": "self.version"}));
        assert_eq!(got[11]["replace"], json!({"lib-zip": "self.version"}));
        assert_eq!(got[8]["replace"], json!({"lib-uuid": "self.version"}));
        let overrides = json!({"php": "8.4.1", "ext-mongodb": false, "ext-gone": "1.0"});
        let got = from_probe(&probe, overrides.as_object()).unwrap();
        assert_eq!(
            names(&got)[..6],
            [
                "php 8.4.1",
                "ext-gone 1.0",
                "composer 2.11.2",
                "composer-plugin-api 2.11.0",
                "composer-runtime-api 2.2.2",
                "php-64bit 8.4.1"
            ]
        );
        assert!(!names(&got).iter().any(|n| n.starts_with("ext-mongodb")));
        assert!(from_probe(&probe, json!({"laravel/x": "1"}).as_object()).is_err());
    }

    #[test]
    fn reads_the_system_php() {
        let got = detect("php", None).unwrap();
        assert!(
            names(&got).iter().any(|n| n.starts_with("php 8.")),
            "{:?}",
            names(&got)
        );
        assert!(detect("/nonexistent/php", None).is_err());
        assert_eq!(
            detect("false", None).unwrap_err().to_string(),
            "false failed to report the platform: "
        );
    }
}
