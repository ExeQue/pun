//! Types against PHPStan 2.3.1 (scripts/export-phpstan-symbols.php): every signature-map type string through
//! `TypeStringResolver` + `describe()`, every type in the reflected signatures through `toPhpDocNode()` and back,
//! and every built-in function's and method's signature on PHP 8.4 and 8.5.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the helpers print the failing case before the test asserts"
)]

use std::collections::HashMap;

use crate::common::fixture;
use pun_phpstan::resolve::resolve_str;
use pun_phpstan::symbols::{self, Function, PhpVersion};
use pun_phpstan::types::{Level, is_super_type_of};

const UNNORMALIZED: &[&str] = &[
    "('apache' | 'apache2handler' | 'cgi' | 'cli' | 'cli-server' | 'embed' | 'fpm-fcgi' | 'litespeed' | 'phpdbg' | non-falsy-string)",
];

const SIGNATURE_GAPS: &[&str] = &[
    "ParentIterator::static",
    "RecursiveCachingIterator::static",
    "RecursiveFilterIterator::static",
    "RecursiveRegexIterator::static",
];

fn skipped(skip: &[&str], ctx: Option<&str>, input: &str) -> bool {
    skip.contains(&input) || ctx.is_some_and(|c| skip.contains(&format!("{c}::{input}").as_str()))
}

fn describe(input: &str, ctx: Option<&str>) -> [String; 3] {
    let ty = resolve_str(input, ctx);
    [Level::Precise, Level::Value, Level::TypeOnly].map(|l| ty.describe(l))
}

type Row = (
    Option<String>,
    String,
    [String; 3],
    Option<String>,
    Option<[String; 2]>,
);

fn rows(name: &str) -> Vec<Row> {
    let f = fixture(name);
    f.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let s = |i: usize| row[i].as_str().unwrap().to_string();
            (
                row[0].as_str().map(str::to_string),
                s(1),
                [s(2), s(3), s(4)],
                row.get(5).and_then(|v| v.as_str()).map(str::to_string),
                row.get(6)
                    .and_then(|v| v.as_array())
                    .map(|_| [0, 1].map(|i| row[6][i].as_str().unwrap().to_string())),
            )
        })
        .collect()
}

fn check(name: &str, skip: &[&str]) {
    let rows = rows(name);
    let mut failures = Vec::new();
    let mut skipped_rows = 0;
    for (ctx, input, want, pun, iterable) in &rows {
        if skipped(skip, ctx.as_deref(), input) {
            skipped_rows += 1;
            continue;
        }
        let ty = resolve_str(pun.as_deref().unwrap_or(input), ctx.as_deref());
        let got = [Level::Precise, Level::Value, Level::TypeOnly].map(|l| ty.describe(l));
        if got != *want {
            failures.push(format!("{ctx:?} {input}\n  want {want:?}\n  got  {got:?}"));
        } else if let Some(want) = iterable {
            let got = [ty.iterable_key_type(), ty.iterable_value_type()]
                .map(|t| t.describe(Level::Precise));
            if got != *want {
                failures.push(format!(
                    "{ctx:?} {input} (iterable key, value)\n  want {want:?}\n  got  {got:?}"
                ));
            }
        }
    }
    eprintln!(
        "{name}: {}/{} match",
        rows.len() - failures.len() - skipped_rows,
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn signature_map_type_strings() {
    check("type-strings.json", &[]);
}

#[test]
fn reflected_types() {
    check(
        "reflected-types.json",
        &[UNNORMALIZED, SIGNATURE_GAPS].concat(),
    );
}

fn signature_strings(f: &Function) -> Vec<&str> {
    let mut strings = Vec::new();
    for v in f.variants.iter().chain(f.named.iter().flatten()) {
        for p in &v.params {
            strings.push(p.ty.as_str());
            strings.extend(p.default.as_deref());
        }
        strings.push(v.ret.as_str());
    }
    strings
}

#[test]
fn built_in_signatures() {
    let reflected: HashMap<(Option<String>, String), [String; 3]> = rows("reflected-types.json")
        .into_iter()
        .map(|(ctx, s, d, ..)| ((ctx, s), d))
        .collect();
    for version in [PhpVersion::new(8, 4), PhpVersion::new(8, 5)] {
        let symbols = symbols::symbols(version);
        let mut callables: Vec<(Option<&str>, &Function)> =
            symbols.functions.values().map(|f| (None, f)).collect();
        for class in symbols.classes.values() {
            callables.extend(
                class
                    .methods
                    .values()
                    .map(|m| (Some(class.name.as_str()), m)),
            );
        }
        let mut failures = Vec::new();
        for (ctx, f) in &callables {
            for s in signature_strings(f) {
                if skipped(SIGNATURE_GAPS, *ctx, s) {
                    continue;
                }
                let want = &reflected[&(ctx.map(str::to_string), s.to_string())];
                let got = describe(s, *ctx);
                if got != *want {
                    failures.push(format!(
                        "{ctx:?} {}: {s}\n  want {want:?}\n  got  {got:?}",
                        f.name
                    ));
                }
            }
        }
        eprintln!(
            "{version:?}: {} functions, {} methods; {} signature types differ",
            symbols.functions.len(),
            callables.len() - symbols.functions.len(),
            failures.len()
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[test]
fn constants() {
    let reflected: HashMap<String, [String; 3]> = rows("reflected-types.json")
        .into_iter()
        .filter(|(ctx, ..)| ctx.is_none())
        .map(|(_, s, d, ..)| (s, d))
        .collect();
    let symbols = symbols::symbols(PhpVersion::new(8, 4));
    let mut failures = Vec::new();
    for (name, ty) in &symbols.constants {
        if !name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || symbols::class(name).is_some()
        {
            continue;
        }
        let got = describe(name, None);
        if got != reflected[ty] {
            failures.push(format!(
                "{name}: {ty}\n  want {:?}\n  got  {got:?}",
                reflected[ty]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const PROBE_GAPS: &[&str] = &[
    "array{'Countable', 'count'}&callable",
    "array<1|2, int>&list",
    "non-empty-mixed",
    "object{a: int}&object{b: int}",
    "(T is int ? string : bool)&int",
    "SimpleXMLElement<int, string, bool>",
    "ArrayIterator<int, covariant string>",
    "ArrayIterator<contravariant int, string>",
    "non-empty-list&array{a?: int}",
    "array<'a', int>&list",
    "list<int>&array{a?: int}",
    "non-empty-list<int>&array{a?: int}",
    "array{a?: int}&list<int>",
    "array{a?: int}&non-empty-list<int>",
    "array|static",
    "array|$this",
    "array{a: int}|static",
    "array{a: int}|$this",
    "list<int>|static",
    "list<int>|$this",
    "non-empty-array<int>|static",
    "non-empty-array<int>|$this",
    "static|array",
    "static|array{a: int}",
    "static|list<int>",
    "static|non-empty-array<int>",
    "$this|array",
    "$this|array{a: int}",
    "$this|list<int>",
    "$this|non-empty-array<int>",
];

const SUPER_GAPS: &[(&str, &str)] = &[
    ("list<int>", "array{a?: int}"),
    ("non-empty-list<int>", "array{a?: int}"),
    ("ArrayIterator<int, int>", "ArrayIterator<array-key, int>"),
];

#[test]
fn probes() {
    check("type-probes.json", PROBE_GAPS);
}

#[test]
fn super_types() {
    let f = fixture("type-super.json");
    let rows = f.as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let [ctx, a, b, want] = [0, 1, 2, 3].map(|i| row[i].as_str());
        if SUPER_GAPS.contains(&(a.unwrap(), b.unwrap())) {
            continue;
        }
        let (a, b) = (resolve_str(a.unwrap(), ctx), resolve_str(b.unwrap(), ctx));
        let got = format!("{:?}", is_super_type_of(&a, &b));
        if got != want.unwrap() {
            failures.push(format!(
                "{ctx:?} {} isSuperTypeOf {}: want {}, got {got}",
                row[1],
                row[2],
                want.unwrap()
            ));
        }
    }
    eprintln!(
        "type-super.json: {}/{} match",
        rows.len() - failures.len(),
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn long_constant_string_truncates_before_punctuation() {
    let ty = resolve_str("'aaaa bbbbbbbbbbbbbb\u{7f}cccc'", None);
    assert_eq!(ty.describe(Level::Value), "'aaaa…'");
}

#[test]
fn constant_array_phpdoc_serialization() {
    let rows = fixture("type-phpdoc-shapes.json");
    let mut failures = Vec::new();
    for row in rows.as_array().unwrap() {
        let input = row[0].as_str().unwrap();
        let got = pun_phpstan::types::to_phpdoc(&resolve_str(input, Some("Exception")));
        let want = row[1].as_str().unwrap();
        if got != want {
            failures.push(format!("{input}\n  want {want:?}\n  got  {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const VERBOSITY_GAPS: &[&str] = &["Traversable<int>"];

const ACCEPT_GAPS: &[(&str, &str)] = &[(
    "array{ArrayIterator<int, string>, ArrayIterator<int, int>}",
    "callable&array",
)];

#[test]
fn template_strict_mixed() {
    use pun_phpstan::types::{StrictMixed, Tri, accepts_with};
    let t = resolve_str("__template<T, 'function probe()', mixed>", None);
    let u = resolve_str("__template<U, 'function probe()', mixed>", None);
    let sm = StrictMixed {
        explicit: true,
        implicit: true,
    };
    assert_eq!(accepts_with(&t, &t, true, sm), Tri::Yes);
    assert_eq!(accepts_with(&u, &t, true, sm), Tri::Maybe);
}

#[test]
fn accepts() {
    use pun_phpstan::types::{StrictMixed, accepts_with, recommended_level};
    let f = fixture("type-accepts.json");
    let rows = f.as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let [ctx, a, b, strict, loose, level] = [0, 1, 2, 3, 4, 5].map(|i| row[i].as_str());
        let at = resolve_str(a.unwrap(), ctx);
        let got_level = match b {
            None => recommended_level(&at, None),
            Some("strict-mixed") => recommended_level(&at, Some(&resolve_str("mixed", None))),
            Some(b) => recommended_level(&at, Some(&resolve_str(b, ctx))),
        };
        let mut got = vec![format!("{got_level:?}")];
        let mut want = vec![level.unwrap().to_string()];
        if let Some(b) = b {
            let (bts, sm) = if b == "strict-mixed" {
                let all = StrictMixed {
                    explicit: true,
                    implicit: true,
                };
                (
                    vec![
                        resolve_str("mixed", None),
                        resolve_str("__template<T, 'function strictProbe()', mixed>", None),
                    ],
                    all,
                )
            } else {
                (vec![resolve_str(b, ctx)], StrictMixed::default())
            };
            for bt in &bts {
                got.push(format!("{:?}", accepts_with(&at, bt, true, sm)));
                got.push(format!("{:?}", accepts_with(&at, bt, false, sm)));
                want.extend([strict, loose].map(|w| w.unwrap().to_string()));
            }
        }
        if VERBOSITY_GAPS.contains(&a.unwrap()) {
            got.remove(0);
            want.remove(0);
        }
        if got != want && !ACCEPT_GAPS.contains(&(a.unwrap(), b.unwrap_or(""))) {
            failures.push(format!(
                "{ctx:?} {} accepts {}: want {want:?}, got {got:?}",
                row[1], row[2]
            ));
        }
    }
    eprintln!(
        "type-accepts.json: {}/{} match",
        rows.len() - failures.len(),
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn casts() {
    use pun_phpstan::types::{Cast, StrictMixed, cast_is_error};
    let f = fixture("type-casts.json");
    let mut failures = Vec::new();
    for row in f.as_array().unwrap() {
        let (ctx, a) = (row[0].as_str(), row[1].as_str().unwrap());
        let (t, sm) = if a == "strict-mixed" {
            (
                resolve_str("mixed", None),
                StrictMixed {
                    explicit: true,
                    implicit: false,
                },
            )
        } else {
            (resolve_str(a, ctx), StrictMixed::default())
        };
        let casts = [
            Cast::String,
            Cast::Number,
            Cast::Integer,
            Cast::Float,
            Cast::BitwiseNot,
        ];
        let got: Vec<bool> = casts.iter().map(|&c| cast_is_error(&t, c, sm)).collect();
        let want: Vec<bool> = (2..7).map(|i| row[i].as_bool().unwrap()).collect();
        if got != want || cast_is_error(&t, Cast::Boolean, sm) {
            failures.push(format!("{ctx:?} {a}: want {want:?}, got {got:?}"));
        }
        let cloneable = format!("{:?}", t.is_cloneable());
        if row[7].as_str().is_some_and(|want| want != cloneable) {
            failures.push(format!(
                "{ctx:?} {a}: isCloneable() want {}, got {cloneable}",
                row[7]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn static_of_unknown_class() {
    use pun_phpstan::types::{Type, accepts};
    let s = Type::Static("Nope".into(), vec![]);
    assert_eq!(format!("{:?}", accepts(&s, &s, true)), "Yes");
}

#[test]
fn infer_template_types() {
    use pun_phpstan::types::{Template, Type, infer_template_types, resolve_template_types};
    let f = fixture("type-infer.json");
    let rows = f.as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        fn collect(t: &Type, out: &std::cell::RefCell<Vec<Template>>) {
            if let Type::Template(tt) = t {
                out.borrow_mut().push((**tt).clone());
            }
            t.traverse(&|i| {
                collect(i, out);
                i.clone()
            });
        }
        let (a, b) = (row[0].as_str().unwrap(), row[1].as_str().unwrap());
        let (at, bt) = (resolve_str(a, None), resolve_str(b, None));
        let map = infer_template_types(&at, &bt);
        let mut got: Vec<(String, String)> = map
            .types()
            .into_iter()
            .map(|(n, _, t)| (n.to_string(), t.describe(Level::Precise)))
            .collect();
        assert!(
            map.types()
                .into_iter()
                .all(|(n, s, t)| (*t == Type::Error) == map.is_absorbed(n, s))
        );
        got.sort();
        let want: Vec<(String, String)> = row[2]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        let templates = std::cell::RefCell::new(Vec::<Template>::new());
        collect(&at, &templates);
        let templates = templates.into_inner();
        let resolved =
            resolve_template_types(&at, &map.with_errors(&templates.iter().collect::<Vec<_>>()))
                .describe(Level::Precise);
        if got != want || resolved != row[3].as_str().unwrap() {
            failures.push(format!(
                "{a} from {b}: want {want:?} {}, got {got:?} {resolved}",
                row[3]
            ));
        }
    }
    eprintln!(
        "type-infer.json: {}/{} match",
        rows.len() - failures.len(),
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn accepts_reasons() {
    use pun_phpstan::types::{StrictMixed, accepts_reasons};
    let f = fixture("type-reasons.json");
    let rows = f.as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let (a, b) = (row[0].as_str().unwrap(), row[1].as_str().unwrap());
        let got = accepts_reasons(
            &resolve_str(a, None),
            &resolve_str(b, None),
            true,
            StrictMixed::default(),
        );
        let want: Vec<String> = row[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().to_string())
            .collect();
        if got != want {
            failures.push(format!("{a} accepts {b}: want {want:?}, got {got:?}"));
        }
    }
    eprintln!(
        "type-reasons.json: {}/{} match",
        rows.len() - failures.len(),
        rows.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn generic_comparisons() {
    use pun_phpstan::types::{StrictMixed, accepts_reasons, accepts_with};
    let fixture = fixture("generic-comparisons.json");
    let mut failures = Vec::new();
    for row in fixture.as_array().unwrap() {
        let a = resolve_str(row[0].as_str().unwrap(), None);
        let received = resolve_str(row[1].as_str().unwrap(), None);
        let checked = row[2].as_bool().unwrap();
        let flags = StrictMixed {
            explicit: checked,
            implicit: checked,
        };
        let b = flags.transform(&received);
        let got = serde_json::json!([
            row[0],
            row[1],
            row[2],
            format!("{:?}", is_super_type_of(&a, &b)),
            format!("{:?}", accepts_with(&a, &received, true, flags)),
            accepts_reasons(&a, &received, true, flags),
        ]);
        if got != *row {
            failures.push(format!("expected {row}\nreceived {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn constant_float_identity() {
    use pun_phpstan::types::{ConstArray, Tri, Type, accepts};
    let shape = |value| {
        Type::ConstArray(Box::new(ConstArray {
            keys: vec![Type::ConstInt(0)],
            values: vec![value],
            next_auto_indexes: vec![1],
            ..ConstArray::empty()
        }))
    };
    for (x, y, equal) in [
        (f64::NAN, f64::NAN, true),
        (f64::NAN, -f64::NAN, true),
        (f64::NAN, 1.0, false),
        (0.0, -0.0, true),
        (f64::INFINITY, f64::INFINITY, true),
        (f64::INFINITY, f64::NEG_INFINITY, false),
    ] {
        let a = Type::ConstFloat(x);
        let b = Type::ConstFloat(y);
        let want = Tri::from(equal);
        assert_eq!(
            (a == b, is_super_type_of(&a, &b), accepts(&a, &b, true)),
            (equal, want, want)
        );
        let (a, b) = (shape(a), shape(b));
        assert_eq!((a == b, is_super_type_of(&a, &b)), (equal, want));
        assert_eq!(accepts(&Type::Union(vec![Type::Int, a]), &b, true), want);
    }
}

#[test]
fn exact_objects_keep_phpstan_equality_containment_and_union_metadata_distinct() {
    fn observation(ty: &Type) -> serde_json::Value {
        fn collect(ty: &Type, exact: &RefCell<Vec<(String, bool)>>) {
            if let Type::Object(name, _, is_exact) = ty {
                exact.borrow_mut().push((name.clone(), *is_exact));
            }
            ty.traverse(&|inner| {
                collect(inner, exact);
                inner.clone()
            });
        }
        let exact = RefCell::new(Vec::new());
        collect(ty, &exact);
        let mut exact = exact.into_inner();
        exact.sort();
        json!([ty.describe(Level::Precise), exact])
    }
    use pun_phpstan::types::{
        Template, Type, accepts, infer_template_types, intersect, mixed, to_phpdoc_fully_qualified,
        union,
    };
    use serde_json::json;
    use std::{cell::RefCell, collections::BTreeMap};

    let object = |name: &str, exact| Type::Object(name.into(), vec![], exact);
    let generic = |exact, item| Type::Object("ArrayObject".into(), vec![Type::Int, item], exact);
    let types = BTreeMap::from([
        ("broad", object("Exception", false)),
        ("exact", object("Exception", true)),
        ("child", object("RuntimeException", true)),
        ("broad-child", object("RuntimeException", false)),
        ("other", object("LogicException", true)),
        ("static", Type::Static("Exception".into(), vec![])),
        ("this", Type::This("Exception".into())),
        ("interface", object("Throwable", false)),
        ("generic-broad", generic(false, object("Exception", false))),
        ("generic-exact", generic(true, object("Exception", false))),
        ("generic-nested", generic(true, object("Exception", true))),
        (
            "excluded",
            Type::object_subtracted(object("Exception", true), object("RuntimeException", false)),
        ),
        (
            "excluded-nested",
            Type::object_subtracted(object("Exception", true), object("RuntimeException", true)),
        ),
        ("cased", object("exception", false)),
        ("cased-exact", object("exception", true)),
        ("final", object("Closure", false)),
        (
            "receiver-conjunction",
            Type::Intersection(vec![
                object("Exception", true),
                Type::This("Exception".into()),
            ]),
        ),
    ]);
    let oracle = fixture("exact-objects.json");
    let mut failures = Vec::new();
    for row in oracle["pairs"].as_array().unwrap() {
        let a = &types[row["left"].as_str().unwrap()];
        let b = &types[row["right"].as_str().unwrap()];
        let got = json!({
            "left": row["left"], "right": row["right"],
            "equals": a == b,
            "super": format!("{:?}", is_super_type_of(a, b)),
            "accepts": format!("{:?}", accepts(a, b, true)),
            "union": observation(&union(vec![a.clone(), b.clone()])),
            "intersection": observation(&intersect(vec![a.clone(), b.clone()])),
        });
        if got != *row {
            failures.push(format!("want {row}\ngot {got}"));
        }
    }
    for (name, want) in oracle["preserved"].as_object().unwrap() {
        let got = observation(&types[name.as_str()].traverse(&Clone::clone));
        if got != *want {
            failures.push(format!("traversed {name}: want {want}\ngot {got}"));
        }
    }
    for (name, want) in oracle["phpdoc"].as_object().unwrap() {
        assert_eq!(
            to_phpdoc_fully_qualified(&types[name.as_str()]),
            want.as_str().unwrap(),
            "{name}"
        );
    }
    let template = Type::Template(Box::new(Template {
        name: "T".into(),
        scope: "function exact()".into(),
        bound: mixed(),
        default: None,
        argument: false,
    }));
    for (name, want) in oracle["inferred"].as_object().unwrap() {
        let inferred = infer_template_types(&template, &types[name.as_str()]);
        let got = observation(inferred.types()[0].2);
        if got != *want {
            failures.push(format!("inferred {name}: want {want}\ngot {got}"));
        }
    }
    for (name, want) in oracle["queries"].as_object().unwrap() {
        let ty = &types[name.as_str()];
        let got = json!([
            format!("{:?}", ty.is_callable()),
            format!("{:?}", ty.has_method("missing"))
        ]);
        if got != *want {
            failures.push(format!("queried {name}: want {want}\ngot {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn compound_equality_keeps_order_for_reflection_not_invariant_generic_identity() {
    use pun_phpstan::types::Type;
    use serde_json::Value;
    let object = |name: &str| Type::Object(name.into(), vec![], false);
    for row in fixture("compound-equality.json").as_array().unwrap() {
        let make = |names: &Value| {
            let members = names
                .as_array()
                .unwrap()
                .iter()
                .map(|n| object(n.as_str().unwrap()))
                .collect();
            match row[0].as_str().unwrap() {
                "union" => Type::Union(members),
                "benevolent" => Type::Benevolent(members),
                "intersection" => Type::Intersection(members),
                _ => unreachable!(),
            }
        };
        let (a, b) = (make(&row[1]), make(&row[2]));
        let generic = |key, value| Type::Object("ArrayIterator".into(), vec![key, value], false);
        assert_eq!(a == b, row[3].as_bool().unwrap(), "{row}");
        assert_eq!(
            generic(object("Key"), a.clone()) == generic(object("Key"), b.clone()),
            row[4].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(
            generic(object("Key"), a.clone()) == generic(b, object("Key")),
            row[5].as_bool().unwrap(),
            "{row}"
        );
        let (Type::Union(members) | Type::Benevolent(members) | Type::Intersection(members)) = a
        else {
            unreachable!()
        };
        assert_eq!(
            members
                .iter()
                .map(Type::object_class_names)
                .collect::<Vec<_>>(),
            row[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| vec![n.as_str().unwrap().to_string()])
                .collect::<Vec<_>>()
        );
    }
}
