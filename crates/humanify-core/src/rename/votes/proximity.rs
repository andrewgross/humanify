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

/// Names always kept regardless of proximity: the CommonJS context
/// (`crate::toolchain::COMMONJS_CONTEXT`, the one list) and these globals.
/// Membership only — the result keeps the caller's name order.
pub fn is_well_known_name(name: &str) -> bool {
    crate::toolchain::is_commonjs_context_name(name) || WELL_KNOWN_GLOBALS.contains(&name)
}

/// The always-kept globals beyond the CommonJS context.
const WELL_KNOWN_GLOBALS: [&str; 27] = [
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
///
/// `is_droppable` keeps the TS's `isEligible` reading — a name the ask
/// could itself rename is not worth the model's avoid-attention — with ONE
/// extension (2026-09-28, the collision fix): the caller must NOT drop a
/// name this run already APPLIED to a binding of the covered scopes
/// (the caller combines eligibility with `RenameState::renamed_names_in`).
/// An eligible-looking word that is taken is not "about to be renamed";
/// dropping it is how the model was never told a natural name was in use.
pub fn get_proximate_used_names<'a, S: AsRef<str>>(
    all_used_names: &'a [S],
    batch_lines: &[u32],
    scope_binding: impl Fn(&str) -> Option<ProximityBinding>,
    total_bindings: usize,
    is_droppable: impl Fn(&str) -> bool,
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
        if is_well_known_name(name) {
            push(&mut result, name);
        }
    }
    // (Renamed.) `preserved`: every name the ask may not silently reuse.
    let preserved: Vec<&str> = names.filter(|n| !is_droppable(n)).collect();
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

/// One scope's windowing inputs as PLAIN data — the extract-then-parallel
/// snapshot the wave processor's module lanes window from
/// (perf-inventory item 2: per-group windowing re-read the same used
/// list, droppability and per-name proximity lines once per group, 73.9 s
/// of a 122.6 s fresh-run round setup).
///
/// [`ProximityWindow::new`] runs on the CALLING thread, where its
/// closures may borrow state that is not `Sync` (the oxc `Semantic` behind
/// the binding tables); [`ProximityWindow::windowed`] is a pure function
/// of the extracted data, so many batches may window in parallel and
/// rejoin in their own order (the `par::map_ordered` rule). The output is
/// byte-identical to [`get_proximate_used_names`] over the same inputs —
/// the serial path's, exactly — pinned in `proximity_test`.
pub struct ProximityWindow {
    /// The used names, in insertion order (the serial `all_used_names`).
    used: Vec<String>,
    /// Whether `used[i]` is well known ([`is_well_known_name`]).
    well_known: Vec<bool>,
    /// The serial `is_droppable` per name: eligibility AND not-taken.
    droppable: Vec<bool>,
    /// The serial `scope_binding(name)` per name, extracted where the
    /// serial loop would consult it: preserved names of a windowed scope
    /// (`total >= WINDOWING_THRESHOLD`); `None` elsewhere — including for
    /// an absent binding, which the serial loop includes "to be safe".
    bindings: Vec<Option<ProximityBinding>>,
    /// The serial `total_bindings`.
    total: usize,
}

impl ProximityWindow {
    /// Extract the snapshot: mirrors [`get_proximate_used_names`]'s read
    /// pattern over the caller's closures — `is_droppable` for every name,
    /// `scope_binding` only for the preserved names a windowed scope
    /// consults.
    pub fn new(
        used: Vec<String>,
        total: usize,
        is_droppable: impl Fn(&str) -> bool,
        scope_binding: impl Fn(&str) -> Option<ProximityBinding>,
    ) -> ProximityWindow {
        let n = used.len();
        let mut well_known = Vec::with_capacity(n);
        let mut droppable = Vec::with_capacity(n);
        for name in &used {
            well_known.push(is_well_known_name(name));
            droppable.push(is_droppable(name));
        }
        let mut bindings = vec![None; n];
        if total >= WINDOWING_THRESHOLD {
            for (i, name) in used.iter().enumerate() {
                if !droppable[i] {
                    bindings[i] = scope_binding(name);
                }
            }
        }
        ProximityWindow {
            used,
            well_known,
            droppable,
            bindings,
            total,
        }
    }

    /// The windowed used names for one batch — the serial
    /// [`get_proximate_used_names`]'s result over the extracted inputs, in
    /// the serial order (well-known first, then preserved names),
    /// deduplicated by insertion. Pure data in, pure data out.
    pub fn windowed(&self, batch_lines: &[u32]) -> Vec<String> {
        fn push_unique<'a>(
            result: &mut Vec<String>,
            members: &mut std::collections::HashSet<&'a str>,
            name: &'a str,
        ) {
            if members.insert(name) {
                result.push(name.to_string());
            }
        }
        let mut result: Vec<String> = Vec::new();
        let mut members: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for i in 0..self.used.len() {
            if self.well_known[i] {
                push_unique(&mut result, &mut members, &self.used[i]);
            }
        }
        if self.total < WINDOWING_THRESHOLD {
            for i in 0..self.used.len() {
                if !self.droppable[i] {
                    push_unique(&mut result, &mut members, &self.used[i]);
                }
            }
            return result;
        }
        // Math.min(...[]) is Infinity and Math.max(...[]) -Infinity: an
        // empty batch windows nothing in (absent bindings still included).
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
        for i in 0..self.used.len() {
            if !self.droppable[i] && in_window(self.bindings[i].as_ref(), min_line, max_line) {
                push_unique(&mut result, &mut members, &self.used[i]);
            }
        }
        result
    }
}

#[cfg(test)]
mod proximity_test;
