//! Typed JSON records written in JavaScript's key order.
//!
//! The pipeline's JSON artifacts (`--stats-json`, `stage-hashes.json`,
//! `placement-stats.json`, the selection record) are TS object literals
//! serialized by `JSON.stringify`, so their KEY ORDER is the runtime
//! construction order — which is NOT always the TS type's declaration order
//! (`ResolutionStats` declares `interchangeableResolved` first and emits it
//! twelfth; `MintedCensus` emits `freeReferences` before `byFamily`). A
//! record here is declared ONCE, fields in the emitted order, and gets:
//!
//! - `to_js` — a [`JsValue`] in that order (absent optionals omitted, as
//!   `JSON.stringify` drops `undefined`), written by `js::stringify*`;
//! - `from_js` — a STRICT reader: a missing required key or an unknown key
//!   is an error, so a round trip over a real artifact proves the shape;
//! - `schema` — the shape as data, compared against the TS checker's view
//!   of the same type (test/parity/wpb4-stats-schema.json).
//!
//! Numbers are JS numbers (`f64`), formatted by `js::number_to_string`.

use crate::js::{JsObject, JsValue};

/// A value's shape — the same vocabulary as the TS schema probe.
#[derive(Clone, Debug, PartialEq)]
pub enum Schema {
    Number,
    String,
    Boolean,
    Array(Box<Schema>),
    /// An index-signature object (`Record<string, number>`).
    Map(Box<Schema>),
    Object(Vec<Prop>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub name: String,
    pub optional: bool,
    pub nullable: bool,
    pub schema: Schema,
}

impl Schema {
    /// The TS probe's JSON form, with object props SORTED by name: the
    /// type's declaration order is not the emitted order, so the schema
    /// comparison is over the property SET (emitted order is proven by the
    /// byte round trip over real artifacts instead).
    pub fn to_probe_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Schema::Number => json!({"kind": "number"}),
            Schema::String => json!({"kind": "string"}),
            Schema::Boolean => json!({"kind": "boolean"}),
            Schema::Array(items) => json!({"kind": "array", "items": items.to_probe_json()}),
            Schema::Map(values) => json!({"kind": "map", "values": values.to_probe_json()}),
            Schema::Object(props) => {
                let mut sorted: Vec<&Prop> = props.iter().collect();
                sorted.sort_by(|a, b| a.name.cmp(&b.name));
                json!({
                    "kind": "object",
                    "props": sorted.iter().map(|p| json!({
                        "name": p.name,
                        "optional": p.optional,
                        "nullable": p.nullable,
                        "type": p.schema.to_probe_json(),
                    })).collect::<Vec<_>>(),
                })
            }
        }
    }
}

/// A JSON value type with a JS representation and a shape.
pub trait JsType: Sized {
    fn to_js(&self) -> JsValue;
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String>;
    fn schema() -> Schema;
}

/// How a record field is present: required, optional (`?:`, omitted when
/// undefined) or nullable (always present, may be `null`).
pub trait JsField: Sized {
    fn put(&self, obj: &mut JsObject, key: &str);
    fn take(obj: &JsObject, key: &str, at: &str) -> Result<Self, String>;
    fn prop(key: &str) -> Prop;
}

impl<T: JsType> JsField for T {
    fn put(&self, obj: &mut JsObject, key: &str) {
        obj.insert(key, self.to_js());
    }
    fn take(obj: &JsObject, key: &str, at: &str) -> Result<Self, String> {
        let v = obj
            .get(key)
            .ok_or_else(|| format!("{at}: missing required key {key:?}"))?;
        T::from_js(v, &format!("{at}.{key}"))
    }
    fn prop(key: &str) -> Prop {
        Prop {
            name: key.to_string(),
            optional: false,
            nullable: false,
            schema: T::schema(),
        }
    }
}

impl<T: JsType> JsField for Option<T> {
    fn put(&self, obj: &mut JsObject, key: &str) {
        if let Some(v) = self {
            obj.insert(key, v.to_js());
        }
    }
    fn take(obj: &JsObject, key: &str, at: &str) -> Result<Self, String> {
        obj.get(key)
            .map(|v| T::from_js(v, &format!("{at}.{key}")))
            .transpose()
    }
    fn prop(key: &str) -> Prop {
        Prop {
            optional: true,
            ..<T as JsField>::prop(key)
        }
    }
}

/// A field that is always written and may be `null` (`x ?? null`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nullable<T>(pub Option<T>);

impl<T: JsType> JsField for Nullable<T> {
    fn put(&self, obj: &mut JsObject, key: &str) {
        obj.insert(key, self.0.as_ref().map_or(JsValue::Null, JsType::to_js));
    }
    fn take(obj: &JsObject, key: &str, at: &str) -> Result<Self, String> {
        match obj.get(key) {
            None => Err(format!("{at}: missing required key {key:?}")),
            Some(JsValue::Null) => Ok(Nullable(None)),
            Some(v) => T::from_js(v, &format!("{at}.{key}")).map(|t| Nullable(Some(t))),
        }
    }
    fn prop(key: &str) -> Prop {
        Prop {
            nullable: true,
            ..<T as JsField>::prop(key)
        }
    }
}

impl JsType for f64 {
    fn to_js(&self) -> JsValue {
        JsValue::Number(*self)
    }
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String> {
        match v {
            JsValue::Number(n) => Ok(*n),
            other => Err(format!("{at}: expected a number, got {other:?}")),
        }
    }
    fn schema() -> Schema {
        Schema::Number
    }
}

impl JsType for String {
    fn to_js(&self) -> JsValue {
        JsValue::String(self.clone())
    }
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String> {
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{at}: expected a string, got {v:?}"))
    }
    fn schema() -> Schema {
        Schema::String
    }
}

impl<T: JsType> JsType for Vec<T> {
    fn to_js(&self) -> JsValue {
        JsValue::Array(self.iter().map(JsType::to_js).collect())
    }
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String> {
        match v {
            JsValue::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, x)| T::from_js(x, &format!("{at}[{i}]")))
                .collect(),
            other => Err(format!("{at}: expected an array, got {other:?}")),
        }
    }
    fn schema() -> Schema {
        Schema::Array(Box::new(T::schema()))
    }
}

/// An index-signature object of numbers, in JS enumeration order (the
/// order the writer inserted — e.g. `reachedSpanBuckets` in bucket order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CountMap(pub Vec<(String, f64)>);

impl CountMap {
    pub fn from_js_map(v: &JsValue, at: &str, allowed: Option<&[&str]>) -> Result<Self, String> {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{at}: expected an object, got {v:?}"))?;
        let mut out = Vec::new();
        for (k, x) in obj.entries() {
            if let Some(allowed) = allowed
                && !allowed.contains(&k.as_str())
            {
                return Err(format!("{at}: unknown key {k:?}"));
            }
            out.push((k.clone(), f64::from_js(x, &format!("{at}.{k}"))?));
        }
        Ok(CountMap(out))
    }

    pub fn to_js_map(&self) -> JsValue {
        let mut obj = JsObject::new();
        for (k, v) in &self.0 {
            obj.insert(k.clone(), JsValue::Number(*v));
        }
        JsValue::Object(obj)
    }
}

impl JsType for CountMap {
    fn to_js(&self) -> JsValue {
        self.to_js_map()
    }
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String> {
        CountMap::from_js_map(v, at, None)
    }
    fn schema() -> Schema {
        Schema::Map(Box::new(Schema::Number))
    }
}

/// Reject keys a strict record does not declare.
pub fn check_keys(obj: &JsObject, allowed: &[&str], at: &str) -> Result<(), String> {
    for (k, _) in obj.entries() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("{at}: unknown key {k:?}"));
        }
    }
    Ok(())
}

/// Declare a record: fields in EMITTED order, each `field: Type = "jsKey"`.
#[macro_export]
macro_rules! js_record {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* $field:ident : $ty:ty = $key:literal ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct $name {
            $( $(#[$fmeta])* pub $field: $ty ),*
        }

        impl $crate::jsshape::JsType for $name {
            fn to_js(&self) -> $crate::js::JsValue {
                let mut obj = $crate::js::JsObject::new();
                $( $crate::jsshape::JsField::put(&self.$field, &mut obj, $key); )*
                $crate::js::JsValue::Object(obj)
            }
            fn from_js(v: &$crate::js::JsValue, at: &str) -> Result<Self, String> {
                let obj = v
                    .as_object()
                    .ok_or_else(|| format!("{at}: expected an object, got {v:?}"))?;
                $crate::jsshape::check_keys(obj, &[$($key),*], at)?;
                Ok($name {
                    $( $field: <$ty as $crate::jsshape::JsField>::take(obj, $key, at)? ),*
                })
            }
            fn schema() -> $crate::jsshape::Schema {
                $crate::jsshape::Schema::Object(vec![
                    $( <$ty as $crate::jsshape::JsField>::prop($key) ),*
                ])
            }
        }
    };
}
