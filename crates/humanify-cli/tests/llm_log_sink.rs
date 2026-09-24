//! The humanify-llm log seam lands in the `-vv` debug logger with the TS
//! formatters (debug.log category lines, the llmRoundtrip block).

use std::sync::{Arc, Mutex};

use humanify_cli::log::{debug_set_output, llm_log_sink, verbose};
use humanify_llm::debug::{LlmLogEvent, Roundtrip};

#[test]
fn llm_events_format_through_the_debug_logger() {
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = lines.clone();
    debug_set_output(Box::new(move |text: &str| {
        sink_lines.lock().unwrap().push(text.to_string());
    }));
    verbose().set_level(2);
    let sink = llm_log_sink();
    sink(LlmLogEvent::Message {
        category: "processor".to_string(),
        message: "llm-cache: write failed for abc: denied".to_string(),
    });
    sink(LlmLogEvent::Roundtrip(Roundtrip {
        method: "suggestAllNames".to_string(),
        model: Some("m".to_string()),
        identifiers: vec!["a".to_string(), "b".to_string()],
        system_prompt: None,
        user_prompt: Some("the prompt".to_string()),
        raw_response: Some("{\"a\":\"x\"}".to_string()),
        duration_ms: 5,
        ok: true,
    }));
    let text = lines.lock().unwrap().join("\n");
    assert!(text.contains("[DEBUG:processor]"), "{text}");
    assert!(
        text.contains("  llm-cache: write failed for abc: denied"),
        "{text}"
    );
    assert!(
        text.contains("[LLM] suggestAllNames - SUCCESS (5ms)"),
        "{text}"
    );
    assert!(text.contains("Identifiers: a, b"), "{text}");
    assert!(text.contains("--- USER PROMPT ---\nthe prompt"), "{text}");
    assert!(
        text.contains("--- RAW RESPONSE ---\n{\"a\":\"x\"}"),
        "{text}"
    );
}
