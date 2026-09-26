//! Proximity windowing for usedNames in large scopes (WP3.3) — TS
//! original: `src/rename/proximity.ts`.
//!
//! When a scope has many bindings, module-level prompts only carry the
//! PRESERVED (non-eligible) names declared or referenced near the batch's
//! lines, plus the well-known globals. Ported here with WP3.3 (the doc-10
//! placement); its consumer is the wave processor's prompt assembly
//! (WP4.3), which supplies the per-name line data.

/// A name's binding as the window reads it: the declaration line and the
/// reference lines (TS `ProximityBinding` — both optional there, `None` /
/// empty here).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProximityBinding {
    pub decl_line: Option<u32>,
    pub ref_lines: Vec<u32>,
}

/// Names always kept regardless of proximity.
pub const WELL_KNOWN_NAMES: [&str; 32] = [
    "exports",
    "require",
    "module",
    "__filename",
    "__dirname",
    "console",
    "process",
    "Buffer",
    "Promise",
    "Object",
    "Array",
    "Map",
    "Set",
    "Error",
    "JSON",
    "Math",
    "undefined",
    "null",
    "NaN",
    "Infinity",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURI",
    "decodeURI",
    "encodeURIComponent",
    "decodeURIComponent",
];

/// Minimum scope bindings before windowing activates.
pub const WINDOWING_THRESHOLD: usize = 100;
/// Line proximity radius.
pub const PROXIMITY_RADIUS: f64 = 100.0;

/// TS `isNameInProximityWindow`.
fn in_window(binding: Option<&ProximityBinding>, min_line: f64, max_line: f64) -> bool {
    let Some(binding) = binding else {
        // Include when the binding is not found, to be safe.
        return true;
    };
    let within = |line: u32| {
        let l = f64::from(line);
        l >= min_line && l <= max_line
    };
    binding.decl_line.is_some_and(within) || binding.ref_lines.iter().any(|&l| within(l))
}

/// TS `getProximateUsedNames`: the windowed usedNames, in insertion order
/// (the TS returns a Set — well-known names first, then the preserved
/// names in `all_used_names` order).
pub fn get_proximate_used_names<'a, S: AsRef<str>>(
    all_used_names: &'a [S],
    batch_lines: &[u32],
    scope_binding: impl Fn(&str) -> Option<ProximityBinding>,
    total_bindings: usize,
    is_eligible: impl Fn(&str) -> bool,
) -> Vec<String> {
    // The Set: insertion-ordered result + a membership index (the waves
    // call this per request over ~25k names — a linear `has` is quadratic).
    let mut result: Vec<String> = Vec::new();
    let mut members: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let names = all_used_names.iter().map(AsRef::as_ref);
    let mut push = |result: &mut Vec<String>, name: &'a str| {
        if members.insert(name) {
            result.push(name.to_string());
        }
    };
    for name in names.clone() {
        if WELL_KNOWN_NAMES.contains(&name) {
            push(&mut result, name);
        }
    }
    let preserved: Vec<&str> = names.filter(|n| !is_eligible(n)).collect();
    if total_bindings < WINDOWING_THRESHOLD {
        for name in preserved {
            push(&mut result, name);
        }
        return result;
    }
    // Math.min(...[]) is Infinity and Math.max(...[]) -Infinity: an empty
    // batch windows nothing in.
    let min_line = batch_lines
        .iter()
        .map(|&l| f64::from(l))
        .fold(f64::INFINITY, f64::min)
        - PROXIMITY_RADIUS;
    let max_line = batch_lines
        .iter()
        .map(|&l| f64::from(l))
        .fold(f64::NEG_INFINITY, f64::max)
        + PROXIMITY_RADIUS;
    for name in preserved {
        // (a member already in the result is skipped — `alreadyIncluded`)
        // `ownEntry(scopeBindings, name)`: an absent name — including one
        // named after an Object.prototype member — is absent, so it is
        // included "to be safe" (16-findings-queue #22, fixed TS-first).
        let binding = scope_binding(name);
        if in_window(binding.as_ref(), min_line, max_line) {
            push(&mut result, name);
        }
    }
    result
}

#[cfg(test)]
mod proximity_test;
