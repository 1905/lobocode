use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Number, Value};

pub fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn integer(n: &Number) -> Option<i128> {
    n.as_i64()
        .map(i128::from)
        .or_else(|| n.as_u64().map(i128::from))
}

fn numbers_equal(a: &Number, b: &Number) -> bool {
    match (integer(a), integer(b)) {
        (Some(a), Some(b)) => a == b,
        (Some(i), None) | (None, Some(i)) => {
            let f = if integer(a).is_none() { a } else { b }.as_f64().unwrap();
            f.fract() == 0.0 && f as i128 == i
        }
        (None, None) => a == b,
    }
}

/// Go nil slices and Rust empty vectors are equivalent. All other nulls are exact.
pub fn assert_json_eq(got: &Value, want: &Value) {
    fn walk(got: &Value, want: &Value, path: &str) {
        match (got, want) {
            (Value::Number(a), Value::Number(b)) => {
                assert!(numbers_equal(a, b), "{path}: {a} != {b}")
            }
            (Value::Null, Value::Array(a)) | (Value::Array(a), Value::Null) if a.is_empty() => {}
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{path}: array length");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    walk(a, b, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(
                    a.keys().collect::<Vec<_>>(),
                    b.keys().collect::<Vec<_>>(),
                    "{path}: keys"
                );
                for (key, a) in a {
                    walk(a, &b[key], &format!("{path}.{key}"));
                }
            }
            _ => assert_eq!(got, want, "{path}"),
        }
    }
    walk(got, want, "$");
}

pub fn round_trip<T: Serialize + DeserializeOwned>(name: &str) -> T {
    let want = fixture(name);
    let decoded: T = serde_json::from_value(want.clone()).unwrap();
    assert_json_eq(&serde_json::to_value(&decoded).unwrap(), &want);
    decoded
}

#[test]
fn equivalent_numbers_and_nil_arrays() {
    assert_json_eq(&serde_json::json!(3), &serde_json::json!(3.0));
    assert_json_eq(&Value::Null, &serde_json::json!([]));
}

#[test]
#[should_panic(expected = "$.a")]
fn mismatch_identifies_field() {
    assert_json_eq(&serde_json::json!({"a":1}), &serde_json::json!({"a":2}));
}

#[test]
fn large_integers_do_not_round_to_equal() {
    let a = serde_json::json!(9007199254740992_u64);
    let b = serde_json::json!(9007199254740993_u64);
    assert!(std::panic::catch_unwind(|| assert_json_eq(&a, &b)).is_err());
    let rounded_max = serde_json::json!(u64::MAX as f64);
    assert!(
        std::panic::catch_unwind(|| assert_json_eq(&serde_json::json!(u64::MAX), &rounded_max))
            .is_err()
    );
}
