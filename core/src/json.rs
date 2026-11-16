//! Short readers for libsimplex JSON. Paths are JSON pointers (`/a/b`).

use serde_json::Value;

pub(crate) trait JsonExt {
    fn str_at(&self, path: &str) -> Option<&str>;
    fn i64_at(&self, path: &str) -> Option<i64>;
    fn bool_at(&self, path: &str) -> Option<bool>;
    fn array_at(&self, path: &str) -> &[Value];
    /// First of `paths` that exists (also when it is `null`).
    fn first(&self, paths: &[&str]) -> Option<&Value>;

    /// The string at `path`, or empty.
    fn text(&self, path: &str) -> String {
        self.str_at(path).unwrap_or_default().to_string()
    }

    /// The trimmed string at `path`, unless missing or blank.
    fn nonempty(&self, path: &str) -> Option<String> {
        self.str_at(path).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
    }

    /// First non-blank string among `paths`.
    fn first_nonempty(&self, paths: &[&str]) -> Option<String> {
        paths.iter().find_map(|p| self.nonempty(p))
    }

    /// The `type` tag of a libsimplex sum type (`{"type": "…", …}`).
    fn tag(&self) -> &str {
        self.str_at("/type").unwrap_or_default()
    }

    /// Whether `path` holds a value other than `null`.
    fn is_set(&self, path: &str) -> bool;
}

impl JsonExt for Value {
    fn str_at(&self, path: &str) -> Option<&str> {
        self.pointer(path)?.as_str()
    }

    fn i64_at(&self, path: &str) -> Option<i64> {
        self.pointer(path)?.as_i64()
    }

    fn bool_at(&self, path: &str) -> Option<bool> {
        self.pointer(path)?.as_bool()
    }

    fn array_at(&self, path: &str) -> &[Value] {
        self.pointer(path).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
    }

    fn first(&self, paths: &[&str]) -> Option<&Value> {
        paths.iter().find_map(|p| self.pointer(p))
    }

    fn is_set(&self, path: &str) -> bool {
        self.pointer(path).is_some_and(|v| !v.is_null())
    }
}
