use super::*;

#[test]
fn the_stub_answers_renamed_on_a_free_port_by_default() {
    let s = start(None).unwrap();
    assert!(s.base_url.starts_with("http://127.0.0.1:"));
    assert!(s.base_url.ends_with("/v1"));
}

#[test]
fn a_port_outside_the_range_is_refused() {
    assert_eq!(
        start(Some(70000)).err().unwrap(),
        "--port 70000 is not a port (1-65535)"
    );
}

#[test]
fn serving_ends_when_its_input_closes() {
    let mut closed: &[u8] = b"anything\n";
    block_until_closed(&mut closed);
}
