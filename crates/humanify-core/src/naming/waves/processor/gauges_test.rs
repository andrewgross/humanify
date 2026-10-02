//! The gauge arithmetic (finding #66's instrumentation): every estimate
//! is DETERMINISTIC and counts every heap allocation ONCE — a String
//! contributes its BUFFER, and the 24-byte header is a slot charged to
//! whatever holds it (a `Vec<String>` pays `size_of::<String>()` per
//! element, a hash table its entry inline size plus a control byte).
//! Allocator slack and growth capacity are deliberately NOT counted, so
//! two identical runs report identical bytes. The formulas below are the
//! contract the owner assembly in [`super::Run::gauges`] leans on.

use std::collections::{HashMap, HashSet};

use super::gauges::{
    WaveGauges, binding_infos_bytes, callee_signatures_bytes, hash_map_strings_to_string,
    hash_map_strings_to_u64, hash_set_of_strings, string_bytes, string_list_bytes,
    taken_snapshot_bytes,
};
use crate::naming::report::{IdentifierOutcome, Outcomes, RenameReport};
use crate::naming::waves::jsset::{JsRecord, JsSet};
use crate::naming::waves::used_set::{NameLayer, UsedSet};
use crate::rename::transfer::owned::BindingInfo;
use crate::rename::validated::scopes::{BScopeId, BindingId};
use humanify_model::llm::CalleeSignature;

#[test]
fn a_string_is_its_header_slot_plus_its_buffer() {
    assert_eq!(
        string_bytes("abc"),
        std::mem::size_of::<String>() as u64 + 3
    );
    assert_eq!(string_bytes(""), std::mem::size_of::<String>() as u64);
}

#[test]
fn a_string_list_is_each_strings_full_footprint() {
    let v = vec!["a".to_string(), "bc".to_string()];
    // The Vec's element slots ARE the String headers.
    assert_eq!(
        string_list_bytes(&v),
        string_bytes("a") + string_bytes("bc")
    );
    assert_eq!(string_list_bytes(&[]), 0);
}

#[test]
fn a_string_hash_set_counts_its_table_and_buffers() {
    let mut s = HashSet::new();
    s.insert("a".to_string());
    // One entry: the (String header) slot + a control byte, plus the
    // name's buffer.
    assert_eq!(
        hash_set_of_strings(&s),
        std::mem::size_of::<String>() as u64 + 1 + 1
    );
    assert_eq!(hash_set_of_strings(&HashSet::new()), 0);
}

#[test]
fn string_keyed_maps_count_their_tables_and_both_buffers() {
    let mut to_u64 = HashMap::new();
    to_u64.insert("ab".to_string(), 1u64);
    assert_eq!(
        hash_map_strings_to_u64(&to_u64),
        std::mem::size_of::<String>() as u64 + std::mem::size_of::<u64>() as u64 + 1 + 2
    );
    let mut to_string = HashMap::new();
    to_string.insert("a".to_string(), "b".to_string());
    assert_eq!(
        hash_map_strings_to_string(&to_string),
        2 * std::mem::size_of::<String>() as u64 + 1 + 1 + 1
    );
}

#[test]
fn a_binding_info_is_its_slot_plus_the_name_buffer() {
    let b = [BindingInfo {
        name: "abc".to_string(),
        binding: BindingId(0),
        scope: BScopeId(0),
    }];
    // The Vec's element slot holds the struct (with the name's header
    // inline); the name's BUFFER is the only extra heap it reaches.
    assert_eq!(
        binding_infos_bytes(&b),
        std::mem::size_of::<BindingInfo>() as u64 + 3
    );
    assert_eq!(binding_infos_bytes(&[]), 0);
}

#[test]
fn a_callee_signature_counts_name_params_and_snippet_buffers() {
    let params = vec!["a".to_string(), "b".to_string()];
    let c = [CalleeSignature {
        name: "f".to_string(),
        params: params.clone(),
        snippet: Some("var".to_string()),
    }];
    // The Vec's element slot holds the struct (name + Option headers
    // inline); the params Vec costs its own slots plus buffers.
    let expected =
        std::mem::size_of::<CalleeSignature>() as u64 + 1 + string_list_bytes(&params) + 3;
    assert_eq!(callee_signatures_bytes(&c), expected);
    // The snippet is Optional: absent is zero extra heap.
    let bare = [CalleeSignature {
        name: "f".to_string(),
        params: Vec::new(),
        snippet: None,
    }];
    assert_eq!(
        callee_signatures_bytes(&bare),
        std::mem::size_of::<CalleeSignature>() as u64 + 1
    );
}

#[test]
fn a_name_layer_counts_its_order_vec_and_its_index_keys() {
    let layer = NameLayer::new(["ab".to_string(), "c".to_string()]);
    // order: a Vec<String> of two header slots plus both buffers; index:
    // a HashMap<String, u32> whose keys CLONE the buffers, plus the table.
    let expected = 2 * std::mem::size_of::<String>() as u64
        + 2
        + 1
        + 2 * (std::mem::size_of::<String>() as u64 + std::mem::size_of::<u32>() as u64 + 1)
        + 2
        + 1;
    assert_eq!(layer.deep_bytes(), expected);
}

#[test]
fn a_used_set_counts_its_own_material_not_the_shared_layers() {
    let layer = NameLayer::new(["a".to_string()]);
    let mut set = UsedSet::new(vec![std::sync::Arc::new(layer)]);
    set.insert("x");
    set.remove("y");
    // The Arc LIST (one pointer per layer), the shadowed Vec per layer
    // (empty here), and the two barrier-edit sets — but NOT the shared
    // layer's contents (the owner assembly counts each layer once, by
    // pointer, across every context that shares it).
    let one = |name: &str| {
        let mut s = HashSet::new();
        s.insert(name.to_string());
        hash_set_of_strings(&s)
    };
    let expected = std::mem::size_of::<std::sync::Arc<NameLayer>>() as u64
        + std::mem::size_of::<Vec<u32>>() as u64
        + one("x")
        + one("y");
    assert_eq!(set.own_bytes(), expected);
}

#[test]
fn a_js_set_counts_its_double_stored_names() {
    let mut s = JsSet::new();
    s.add("ab");
    s.add("cd");
    // pos: a HashMap<String, u64> (the name once); order: a BTreeMap<u64,
    // String> (the name AGAIN — the Set stores every member twice).
    let expected = 2
        * (std::mem::size_of::<String>() as u64 + std::mem::size_of::<u64>() as u64 + 1)
        + 2
        + 2
        + 2 * (std::mem::size_of::<u64>() as u64 + std::mem::size_of::<String>() as u64 + 16)
        + 2
        + 2;
    assert_eq!(s.deep_bytes(), expected);
}

#[test]
fn a_js_record_counts_its_pair_slots_and_buffers() {
    let mut r = JsRecord::default();
    r.set("ab", "cdef");
    let expected = std::mem::size_of::<(String, String)>() as u64 + 2 + 4;
    assert_eq!(r.deep_bytes(), expected);
}

#[test]
fn a_rename_report_counts_its_buffers_trails_and_slots() {
    let mut outcomes = Outcomes::default();
    outcomes.set("ab", IdentifierOutcome::renamed("xNamed", 1, None));
    let report = RenameReport {
        ty: crate::naming::report::ReportType::Function,
        strategy: crate::naming::report::ReportStrategy::Llm,
        target_id: "fn".to_string(),
        total_identifiers: 1,
        renamed_count: 1,
        outcomes,
        total_llm_calls: Some(2),
        finish_reasons: vec![None, Some("stop".to_string())],
        structural_hash: None,
    };
    // target_id/structural_hash ride the report's own slot (buffer only);
    // the outcomes Vec pays (String, IdentifierOutcome) per pair plus the
    // key's and the renamed name's buffers; the finish-reasons Vec pays
    // size_of::<Option<String>>() per slot plus the Some buffers.
    let expected = 2
        + std::mem::size_of::<(String, IdentifierOutcome)>() as u64
        + 2
        + 6
        + 2 * std::mem::size_of::<Option<String>>() as u64
        + 4;
    assert_eq!(report.deep_bytes(), expected);
}

#[test]
fn the_gauges_default_to_zero() {
    assert_eq!(
        WaveGauges::default(),
        WaveGauges {
            strategy_bytes: 0,
            strategy_bindings_bytes: 0,
            strategy_taken_bytes: 0,
            strategy_callee_bytes: 0,
            strategy_callsite_bytes: 0,
            strategy_context_var_bytes: 0,
            strategy_module_bytes: 0,
            ctx_bytes: 0,
            taken_set_names: 0,
            used_set_bytes: 0,
            name_record_bytes: 0,
            bookkeeping_bytes: 0,
        }
    );
}

/// The strategy split's taken term (finding #66's taken-set follow-up):
/// a snapshot is charged ONCE, by pointer, however many strategies hold
/// it — and NEVER for the copy the `renamed_layers` map already charges
/// to `used_set_bytes` (its current per-scope snapshots).
#[test]
fn a_taken_snapshot_is_charged_once_and_never_for_the_maps_copy() {
    use std::sync::Arc;
    let names = |n: &str| {
        let mut s = HashSet::new();
        s.insert(n.to_string());
        s
    };
    let map_resident = Arc::new(names("a"));
    let private = Arc::new(names("b"));
    let mut map_ptrs: HashSet<*const HashSet<String>> = HashSet::new();
    map_ptrs.insert(Arc::as_ptr(&map_resident));
    let mut seen: HashSet<*const HashSet<String>> = HashSet::new();
    // The map holds it: `used_set_bytes` already counts these bytes.
    assert_eq!(
        taken_snapshot_bytes(
            &map_resident,
            Arc::as_ptr(&map_resident),
            &map_ptrs,
            &mut seen
        ),
        0
    );
    // A strategy's own snapshot: full bytes, the first time only.
    let ptr = Arc::as_ptr(&private);
    assert_eq!(
        taken_snapshot_bytes(&private, ptr, &map_ptrs, &mut seen),
        hash_set_of_strings(&private)
    );
    // A second strategy sharing that snapshot: nothing more.
    assert_eq!(taken_snapshot_bytes(&private, ptr, &map_ptrs, &mut seen), 0);
}
