use super::Lanes;

#[test]
fn lines_print_in_slot_order_whatever_order_the_slots_finish() {
    let l = Lanes::new(3, false);
    l.say(2, "c1".into());
    l.say(1, "b1".into());
    l.say(0, "a1".into());
    l.finish(2);
    l.say(1, "b2".into());
    l.finish(1);
    l.say(0, "a2".into());
    l.finish(0);
    assert_eq!(l.close(), ["a1", "a2", "b1", "b2", "c1"]);
}

#[test]
fn one_lane_prints_every_line_as_it_is_said() {
    let l = Lanes::new(2, false);
    l.say(0, "a".into());
    l.finish(0);
    l.say(1, "b".into());
    let st = l.state.lock().unwrap();
    assert_eq!(st.printed, ["a", "b"], "the head slot never waits");
}

#[test]
fn a_line_said_after_its_slot_was_released_is_not_lost() {
    let l = Lanes::new(2, false);
    l.finish(0);
    l.finish(1);
    l.say(0, "late".into());
    assert_eq!(l.close(), ["late"]);
}
