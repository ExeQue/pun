//! Larastan's SquashedMigrationHelper reads `database/schema` dumps with `IamcalSqlParser`, its wrapper of
//! iamcal/sql-parser (the parser it uses unless phpmyadmin/sql-parser is installed). This ports the part of
//! `SQLParser::parse()` that `IamcalSqlParser::parseTables()` reads: each `CREATE TABLE`'s columns, with their type,
//! `UNSIGNED`, nullability and enum values. A syntax error the parser throws on (`throw_on_bad_syntax`) fails the
//! whole file, as `SqlParserFailure` does.

pub(super) struct Column {
    pub name: String,
    /// `MySqlDataTypeToPhpTypeConverter::convert()`'s type, with `enum` for an enum's literal strings
    pub readable_type: String,
    pub nullable: bool,
    pub values: Vec<String>,
}

pub(super) struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

/// `IamcalSqlParser::parseTables()`: the tables in `$tables` order, or `None` where it throws.
pub(super) fn parse_tables(sql: &[u8]) -> Option<Vec<Table>> {
    let source_map = lex(sql)?;
    let tokens = extract_tokens(sql, &source_map);
    let mut tables: Vec<(String, Table)> = Vec::new();
    for statement in tokens.split(|t| t == ";").filter(|s| !s.is_empty()) {
        if !matches!(
            statement[0].to_ascii_uppercase().as_str(),
            "CREATE TABLE" | "CREATE TEMPORARY TABLE"
        ) {
            continue;
        }
        let Some((key, table)) = parse_create_table(statement)? else {
            continue;
        };
        // `$tables[$this->generateTableKey($table)] = $table`: a repeated key keeps its first position
        match tables.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = table,
            None => tables.push((key, table)),
        }
    }
    Some(tables.into_iter().map(|(_, t)| t).collect())
}

/// `SQLParser::_lex()`: each token's `(position, length)`, or `None` for an unterminated comment, backtick or string.
fn lex(sql: &[u8]) -> Option<Vec<(usize, usize)>> {
    let len = sql.len();
    let mut pos = 0;
    let mut map = Vec::new();
    let find = |from: usize, needle: &[u8]| {
        (from <= len)
            .then(|| sql[from..].windows(needle.len()).position(|w| w == needle))
            .flatten()
            .map(|p| p + from)
    };
    while pos < len {
        let c = sql[pos];
        // `!\s+!A`
        if matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r') {
            pos += 1;
        } else if sql[pos..].starts_with(b"--") {
            pos = find(pos, b"\n")? + 1;
        } else if sql[pos..].starts_with(b"/*") {
            pos = find(pos, b"*/")? + 2;
        } else if c.is_ascii_alphabetic() {
            // `![[:alpha:]][[:alnum:]_]*!A`
            let end = (pos + 1..len)
                .find(|&i| !(sql[i].is_ascii_alphanumeric() || sql[i] == b'_'))
                .unwrap_or(len);
            map.push((pos, end - pos));
            pos = end;
        } else if c == b'`' {
            let end = find(pos + 1, b"`")?;
            map.push((pos, 1 + end - pos));
            pos = end + 1;
        } else if let Some(n) = number(&sql[pos..]) {
            map.push((pos, n));
            pos += n;
        } else if c == b'\'' || c == b'"' {
            let mut i = pos + 1;
            loop {
                match sql.get(i) {
                    None => return None,
                    Some(b'\\') => i += 2,
                    Some(&q) if q == c && sql.get(i + 1) == Some(&c) => i += 2,
                    Some(&q) if q == c => break,
                    Some(_) => i += 1,
                }
            }
            map.push((pos, i + 1 - pos));
            pos = i + 1;
        } else {
            map.push((pos, 1));
            pos += 1;
        }
    }
    Some(map)
}

/// The length `![-+]?(\d+\.?\d*|\.\d+)!A` matches at the start of `s`.
fn number(s: &[u8]) -> Option<usize> {
    let sign = usize::from(matches!(s.first(), Some(b'-' | b'+')));
    let digits = |from: usize| s[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let whole = digits(sign);
    if whole > 0 {
        let mut end = sign + whole;
        if s.get(end) == Some(&b'.') {
            end += 1 + digits(end + 1);
        }
        Some(end)
    } else if s.get(sign) == Some(&b'.') && digits(sign + 1) > 0 {
        Some(sign + 1 + digits(sign + 1))
    } else {
        None
    }
}

/// `SQLParser::_extract_tokens()`: the tokens, with the multi-word keywords joined and the single keywords uppercased.
fn extract_tokens(sql: &[u8], source_map: &[(usize, usize)]) -> Vec<String> {
    const LISTS: &[&str] = &[
        "FULLTEXT INDEX",
        "FULLTEXT KEY",
        "SPATIAL INDEX",
        "SPATIAL KEY",
        "FOREIGN KEY",
        "USING BTREE",
        "USING HASH",
        "PRIMARY KEY",
        "UNIQUE INDEX",
        "UNIQUE KEY",
        "CREATE TABLE",
        "CREATE TEMPORARY TABLE",
        "DATA DIRECTORY",
        "INDEX DIRECTORY",
        "DEFAULT CHARACTER SET",
        "CHARACTER SET",
        "DEFAULT CHARSET",
        "DEFAULT COLLATE",
        "IF NOT EXISTS",
        "NOT NULL",
        "WITH PARSER",
        "MATCH FULL",
        "MATCH PARTIAL",
        "MATCH SIMPLE",
        "ON DELETE",
        "ON UPDATE",
        "SET NULL",
        "NO ACTION",
        "SET DEFAULT",
        "DOUBLE PRECISION",
        "CHARACTER VARYING",
    ];
    const SINGLES: &[&str] = &["NULL", "CONSTRAINT", "INDEX", "KEY", "UNIQUE"];
    let token = |i: usize| -> String {
        let (start, length) = source_map[i];
        String::from_utf8_lossy(&sql[start..start + length]).into_owned()
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < source_map.len() {
        let upper = token(i).to_ascii_uppercase();
        let joined = LISTS.iter().find(|list| {
            let words: Vec<&str> = list.split(' ').collect();
            words[0] == upper
                && words.iter().enumerate().all(|(k, word)| {
                    i + k < source_map.len() && token(i + k).to_ascii_uppercase() == *word
                })
        });
        if let Some(list) = joined {
            out.push((*list).to_string());
            i += list.split(' ').count();
        } else if SINGLES.contains(&upper.as_str()) {
            out.push(upper);
            i += 1;
        } else {
            out.push(token(i));
            i += 1;
        }
    }
    out
}

/// `SQLParser::parse_create_table()` from the token after `CREATE TABLE`: the table key and the table, `Ok(None)` for
/// a statement `IamcalSqlParser` skips (no name, or `CREATE TABLE ... LIKE`), `None` where the parser throws.
#[expect(
    clippy::option_option,
    reason = "a skipped statement and a failed file differ"
)]
fn parse_create_table(tokens: &[String]) -> Option<Option<(String, Table)>> {
    let mut i = 1;
    if tokens.get(i).is_some_and(|t| t == "IF NOT EXISTS") {
        i += 1;
    }
    let Some(first) = tokens.get(i) else {
        return Some(None);
    };
    let mut name = decode_identifier(first);
    let mut key = name.clone();
    i += 1;
    if tokens.get(i).is_some_and(|t| t == ".") {
        i += 1;
        let Some(table) = tokens.get(i) else {
            return Some(None);
        };
        name = decode_identifier(table);
        key = format!("{key}.{name}");
        i += 1;
    }
    if tokens
        .get(i)
        .is_some_and(|t| t.eq_ignore_ascii_case("LIKE"))
    {
        return Some(None);
    }
    let mut columns = Vec::new();
    if tokens.get(i).is_some_and(|t| t == "(") {
        i += 1;
        while i < tokens.len() && tokens[i] != ")" {
            let field = slice_until_next_field(tokens, &mut i);
            if let Some(column) = parse_field_or_key(field)? {
                columns.push(column);
            }
        }
    }
    Some(Some((key, Table { name, columns })))
}

/// `SQLParser::slice_until_next_field()`
fn slice_until_next_field<'t>(tokens: &'t [String], i: &mut usize) -> &'t [String] {
    let start = *i;
    let mut depth = 0usize;
    while *i < tokens.len() {
        match tokens[*i].as_str() {
            "(" => depth += 1,
            ")" if depth == 0 => return &tokens[start..*i],
            ")" => depth -= 1,
            "," if depth == 0 => {
                *i += 1;
                return &tokens[start..*i - 1];
            }
            _ => {}
        }
        *i += 1;
    }
    &tokens[start..]
}

/// The token at `i`, or the empty string past the end.
fn at(tokens: &[String], i: usize) -> &str {
    tokens.get(i).map_or("", String::as_str)
}

/// `SQLParser::parse_field_or_key()`: the column a definition declares, `Ok(None)` for an index or a check.
#[expect(clippy::option_option, reason = "a key and a failed file differ")]
fn parse_field_or_key(mut tokens: &[String]) -> Option<Option<Column>> {
    if at(tokens, 0) == "CONSTRAINT" {
        let unnamed = matches!(
            at(tokens, 1),
            "PRIMARY KEY" | "UNIQUE" | "UNIQUE KEY" | "UNIQUE INDEX" | "FOREIGN KEY"
        );
        tokens = &tokens[(if unnamed { 1 } else { 2 }).min(tokens.len())..];
    }
    match at(tokens, 0) {
        "INDEX" | "KEY" | "UNIQUE" | "UNIQUE INDEX" | "UNIQUE KEY" | "PRIMARY KEY" | "FULLTEXT"
        | "FULLTEXT INDEX" | "FULLTEXT KEY" | "SPATIAL" | "SPATIAL INDEX" | "SPATIAL KEY"
        | "FOREIGN KEY" | "CHECK" => Some(None),
        _ => parse_field(tokens).map(Some),
    }
}

/// `SQLParser::parse_field()`, the parts `IamcalSqlParser` reads; `None` for an unsupported type.
fn parse_field(tokens: &[String]) -> Option<Column> {
    let mut rest = tokens.get(2..).unwrap_or_default();
    let name = tokens
        .first()
        .map(|t| decode_identifier(t))
        .unwrap_or_default();
    let ty = tokens
        .get(1)
        .map(|t| t.to_ascii_uppercase())
        .unwrap_or_default();
    let upper_is = |rest: &[String], word: &str| at(rest, 0).eq_ignore_ascii_case(word);
    // parse_field_length(), and parse_field_length_decimals()
    let length = |rest: &mut &[String]| {
        if rest.len() >= 3 && at(rest, 0) == "(" && at(rest, 2) == ")" {
            *rest = &rest[3..];
        }
    };
    let decimals = |rest: &mut &[String]| {
        if rest.len() >= 5 && at(rest, 0) == "(" && at(rest, 2) == "," && at(rest, 4) == ")" {
            *rest = &rest[5..];
        }
    };
    let word = |rest: &mut &[String], word: &str| {
        let found = upper_is(rest, word);
        if found {
            *rest = &rest[1..];
        }
        found
    };
    // parse_field_charset() and parse_field_collate() take the word and the name after it
    let named = |rest: &mut &[String], word: &str| {
        if upper_is(rest, word) {
            *rest = rest.get(2..).unwrap_or_default();
        }
    };
    let mut unsigned = false;
    let mut values = Vec::new();
    match ty.as_str() {
        "DATE" | "YEAR" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" | "JSON" | "GEOMETRY"
        | "POINT" | "LINESTRING" | "POLYGON" | "MULTIPOINT" | "MULTILINESTRING"
        | "MULTIPOLYGON" | "GEOMETRYCOLLECTION" | "BOOLEAN" | "BOOL" => {}
        "TIME" | "TIMESTAMP" | "DATETIME" => length(&mut rest),
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" => {
            length(&mut rest);
            unsigned = word(&mut rest, "UNSIGNED");
            word(&mut rest, "ZEROFILL");
        }
        "REAL" | "DOUBLE" | "DOUBLE PRECISION" | "FLOAT" => {
            decimals(&mut rest);
            unsigned = word(&mut rest, "UNSIGNED");
            word(&mut rest, "ZEROFILL");
        }
        "DECIMAL" | "NUMERIC" | "DEC" | "FIXED" => {
            decimals(&mut rest);
            length(&mut rest);
            unsigned = word(&mut rest, "UNSIGNED");
            word(&mut rest, "ZEROFILL");
        }
        "BIT" | "BINARY" | "VARBINARY" => length(&mut rest),
        "CHAR" | "VARCHAR" | "CHARACTER VARYING" => {
            word(&mut rest, "BINARY");
            length(&mut rest);
            named(&mut rest, "CHARACTER SET");
            named(&mut rest, "COLLATE");
        }
        "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" => {
            word(&mut rest, "BINARY");
            named(&mut rest, "CHARACTER SET");
            named(&mut rest, "COLLATE");
        }
        "ENUM" | "SET" => {
            values = parse_value_list(&mut rest);
            named(&mut rest, "CHARACTER SET");
            named(&mut rest, "COLLATE");
        }
        _ => return None,
    }
    // `$field['null'] ?? true`
    let mut nullable = true;
    if word(&mut rest, "NOT NULL") {
        nullable = false;
    }
    if word(&mut rest, "NULL") {
        nullable = true;
    }
    if upper_is(rest, "DEFAULT") && rest.get(1).is_some_and(|v| decode_value(v) == "NULL") {
        nullable = true;
    }
    Some(Column {
        name,
        readable_type: readable_type(&ty, unsigned, &values),
        nullable,
        values,
    })
}

/// `SQLParser::parse_value_list()`: the values, empty where it returns null.
fn parse_value_list(rest: &mut &[String]) -> Vec<String> {
    if rest.first().is_none_or(|t| t != "(") {
        return Vec::new();
    }
    *rest = &rest[1..];
    let mut values = Vec::new();
    while let Some(first) = rest.first() {
        if first == ")" {
            *rest = &rest[1..];
            return values;
        }
        values.push(decode_value(first));
        *rest = &rest[1..];
        match rest.first().map(String::as_str) {
            Some(")") => {
                *rest = &rest[1..];
                return values;
            }
            Some(",") => *rest = &rest[1..],
            _ => return values,
        }
    }
    Vec::new()
}

/// `SQLParser::decode_identifier()`
fn decode_identifier(token: &str) -> String {
    match token.strip_prefix('`') {
        Some(rest) => rest[..rest.len().saturating_sub(1)].to_string(),
        None => token.to_string(),
    }
}

/// `SQLParser::decode_value()`
fn decode_value(token: &str) -> String {
    let bytes = token.as_bytes();
    let Some(&q) = bytes.first().filter(|q| matches!(q, b'\'' | b'"')) else {
        return token.to_string();
    };
    let mut out = Vec::new();
    let mut i = 1;
    while i + 1 < bytes.len() {
        let next = bytes[i + 1];
        if bytes[i] == b'\\' {
            out.push(match next {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                other => other,
            });
            i += 1;
        } else if bytes[i] == q && next == q {
            out.push(q);
            i += 1;
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `MySqlDataTypeToPhpTypeConverter::convert()`, with `enum` for the union of the values' literal strings.
fn readable_type(ty: &str, unsigned: bool, values: &[String]) -> String {
    match ty {
        "CHAR" | "VARCHAR" | "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "BINARY"
        | "VARBINARY" | "DATE" | "DATETIME" | "TIMESTAMP" | "TIME" | "TINYBLOB" | "BLOB"
        | "MEDIUMBLOB" | "JSON" => "string",
        "BIT" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" | "YEAR" => {
            if unsigned {
                "non-negative-int"
            } else {
                "int"
            }
        }
        "DECIMAL" | "DEC" | "NUMERIC" | "FIXED" | "FLOAT" | "DOUBLE" | "DOUBLE PRECISION"
        | "REAL" => "float",
        "BOOL" | "BOOLEAN" => "bool",
        "ENUM" if !values.is_empty() => "enum",
        _ => "mixed",
    }
    .to_string()
}
