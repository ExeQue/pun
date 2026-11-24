//! PHPStan's `ArraySumFunctionDynamicReturnTypeExtension`: `array_sum()` is the sum of the values (each converted
//! with `toNumber()`, so `null` and `false` add 0 and `true` adds 1), plus 0 when the array may be empty. Only
//! values that are `int`, `float`, `bool` or `null` are typed here; any other value keeps the stub's return type.

use std::collections::BTreeSet;

use mago_analyzer::plugin::context::{InvocationInfo, ProviderContext};
use mago_analyzer::plugin::provider::function::{FunctionReturnTypeProvider, FunctionTarget};
use mago_analyzer::plugin::provider::{Provider, ProviderMeta};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::atomic::scalar::int::TInteger;
use mago_codex::ttype::union::TUnion;
use mago_codex::ttype::{get_float, get_int_range, get_literal_int};

static META: ProviderMeta =
    ProviderMeta::new("pun-array-sum", "pun", "PHPStan's array_sum() return type");

pub(super) struct ArraySum;

impl Provider for ArraySum {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

impl FunctionReturnTypeProvider for ArraySum {
    fn targets() -> FunctionTarget {
        FunctionTarget::Exact(b"array_sum")
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        let argument = context.get_expression_type(invocation.get_argument(0, &[b"array"])?)?;
        let mut result: Option<Sum> = None;
        for atomic in argument.types.iter() {
            let TAtomic::Array(array) = atomic else {
                return None;
            };
            let sum = sum(array)?;
            result = Some(match result {
                Some(all) => all.union(sum),
                None => sum,
            });
        }
        Some(result?.to_union())
    }
}

/// InitializerExprTypeResolver::CALCULATE_SCALARS_LIMIT: the most constant results a sum keeps apart.
const CALCULATE_SCALARS_LIMIT: usize = 128;

/// The integer part of a number type: its constant values, or a range whose `None` bounds are unbounded.
#[derive(Clone)]
enum Int {
    Values(BTreeSet<i64>),
    Range(Option<i64>, Option<i64>),
}

impl Int {
    fn zero() -> Int {
        Int::Values(BTreeSet::from([0]))
    }

    fn bounds(&self) -> (Option<i64>, Option<i64>) {
        match self {
            Int::Values(values) => (values.first().copied(), values.last().copied()),
            Int::Range(min, max) => (*min, *max),
        }
    }

    fn union(self, other: Int) -> Int {
        match (self, other) {
            (Int::Values(mut a), Int::Values(b)) => {
                a.extend(b);
                Int::Values(a)
            }
            (a, b) => {
                let ((a, b), (c, d)) = (a.bounds(), b.bounds());
                Int::Range(
                    a.zip(c).map(|(a, c)| a.min(c)),
                    b.zip(d).map(|(b, d)| b.max(d)),
                )
            }
        }
    }

    fn plus(&self, other: &Int) -> Int {
        if let (Int::Values(a), Int::Values(b)) = (self, other)
            && a.len() * b.len() <= CALCULATE_SCALARS_LIMIT
            && let Some(values) = a
                .iter()
                .flat_map(|a| b.iter().map(move |b| a.checked_add(*b)))
                .collect::<Option<BTreeSet<_>>>()
        {
            return Int::Values(values);
        }
        let ((a, b), (c, d)) = (self.bounds(), other.bounds());
        Int::Range(
            a.zip(c).and_then(|(a, c)| a.checked_add(c)),
            b.zip(d).and_then(|(b, d)| b.checked_add(d)),
        )
    }
}

/// A number type: its integer part, if any, and whether it may be a float.
#[derive(Clone)]
struct Sum {
    int: Option<Int>,
    float: bool,
}

impl Sum {
    fn include(&mut self, int: Int) {
        self.int = Some(match self.int.take() {
            None => int,
            Some(all) => all.union(int),
        });
    }

    fn union(mut self, other: Sum) -> Sum {
        if let Some(int) = other.int {
            self.include(int);
        }
        self.float |= other.float;
        self
    }

    /// `int + int` is an int, and either side being a float makes a float.
    fn plus(&self, other: &Sum) -> Sum {
        Sum {
            int: self
                .int
                .as_ref()
                .zip(other.int.as_ref())
                .map(|(a, b)| a.plus(b)),
            float: self.float || other.float,
        }
    }

    fn to_union(&self) -> TUnion {
        let mut types = Vec::new();
        match &self.int {
            Some(Int::Values(values)) => {
                for value in values {
                    types.extend(get_literal_int(*value).types.iter().cloned());
                }
            }
            Some(Int::Range(min, max)) => {
                let int = match (min, max) {
                    (Some(min), Some(max)) if min == max => get_literal_int(*min),
                    _ => get_int_range(*min, *max),
                };
                types.extend(int.types.iter().cloned());
            }
            None => {}
        }
        if self.float {
            types.extend(get_float().types.iter().cloned());
        }
        TUnion::from_vec(types)
    }
}

/// `toNumber()` of an array value, or `None` when it is anything but `int`, `float`, `bool` or `null`.
fn number(t: &TUnion) -> Option<Sum> {
    let mut out = Sum {
        int: None,
        float: false,
    };
    for atomic in t.types.iter() {
        let int = match atomic {
            TAtomic::Null => Int::zero(),
            TAtomic::Scalar(TScalar::Bool(b)) => match b.value {
                Some(v) => Int::Values(BTreeSet::from([i64::from(v)])),
                None => Int::Values(BTreeSet::from([0, 1])),
            },
            TAtomic::Scalar(TScalar::Integer(TInteger::Literal(v))) => {
                Int::Values(BTreeSet::from([*v]))
            }
            TAtomic::Scalar(TScalar::Integer(i)) => {
                let (min, max) = i.get_bounds();
                Int::Range(min, max)
            }
            TAtomic::Scalar(TScalar::Float(_)) => {
                out.float = true;
                continue;
            }
            _ => return None,
        };
        out.include(int);
    }
    Some(out)
}

fn sum(array: &TArray) -> Option<Sum> {
    // A constant array's values, or for any other array its general value type (`getIterableValueType()`, which
    // includes the values of its known keys).
    let (known, general): (Vec<(bool, &TUnion)>, Option<&TUnion>) = match array {
        TArray::Keyed(keyed) => (
            keyed
                .known_items
                .iter()
                .flat_map(|i| i.values())
                .map(|(o, v)| (*o, v))
                .collect(),
            keyed.parameters.as_ref().map(|(_, value)| &**value),
        ),
        TArray::List(list) => (
            list.known_elements
                .iter()
                .flat_map(|i| i.values())
                .map(|(o, v)| (*o, v))
                .collect(),
            (!list.element_type.is_never()).then_some(&*list.element_type),
        ),
    };
    let non_empty = array.is_non_empty() || known.iter().any(|(optional, _)| !optional);
    let mut out = if let Some(general) = general {
        // `$item + $item * int<0, max>`.
        let mut item = number(general)?;
        for (_, value) in &known {
            item = item.union(number(value)?);
        }
        let times = Sum {
            int: item.int.as_ref().map(|int| {
                let (min, max) = int.bounds();
                Int::Range(
                    min.filter(|m| *m >= 0).map(|_| 0),
                    max.filter(|m| *m <= 0).map(|_| 0),
                )
            }),
            float: item.float,
        };
        item.plus(&times)
    } else {
        // `0 + ...` each value, an optional one adding its value or 0.
        let mut total = Sum {
            int: Some(Int::zero()),
            float: false,
        };
        for (optional, value) in known {
            let mut value = number(value)?;
            if optional {
                value.include(Int::zero());
            }
            total = total.plus(&value);
        }
        total
    };
    if !non_empty {
        out.include(Int::zero());
    }
    Some(out)
}
