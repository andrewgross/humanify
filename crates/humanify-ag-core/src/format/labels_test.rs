use super::{FormatOptions, format};

fn formatted(code: &str) -> String {
    format(code, &FormatOptions::default()).unwrap()
}

#[test]
fn a_label_is_named_for_the_statement_it_marks() {
    let out = formatted(
        "a:for(;;){b:switch(x){case 1:break b;default:continue a}c:{if(y)break c}break a}",
    );
    assert!(out.contains("loop: for (;;)"), "{out}");
    assert!(out.contains("choice: switch (x)"), "{out}");
    assert!(out.contains("break choice;"), "{out}");
    assert!(out.contains("continue loop;"), "{out}");
    assert!(out.contains("block: {"), "{out}");
    assert!(out.contains("break block;"), "{out}");
    assert!(out.contains("break loop;"), "{out}");
}

#[test]
fn a_label_inside_one_of_the_same_kind_is_numbered() {
    let out =
        formatted("q:for(;;){r:while(x){s:do{if(y)continue q;if(z)break r;break s}while(w)}}");
    assert!(out.contains("loop: for (;;)"), "{out}");
    assert!(out.contains("loop2: while (x)"), "{out}");
    assert!(out.contains("loop3: do {"), "{out}");
    assert!(out.contains("continue loop;"), "{out}");
    assert!(out.contains("break loop2;"), "{out}");
    assert!(out.contains("break loop3;"), "{out}");
}

#[test]
fn a_functions_labels_are_numbered_on_their_own() {
    let out = formatted("a:for(;;){f(function(){b:for(;;){break b}});break a}");
    assert_eq!(out.matches("loop: for (;;)").count(), 2, "{out}");
    assert!(!out.contains("loop2"), "{out}");
}

#[test]
fn the_same_code_under_other_label_names_formats_the_same() {
    let one = formatted("Zi:switch(x){case 1:break Zi}");
    let two = formatted("Yi:switch(x){case 1:break Yi}");
    assert_eq!(one, two);
}

#[test]
fn labels_keep_their_names_when_the_option_is_off() {
    let opts = FormatOptions {
        label_names: false,
        ..FormatOptions::default()
    };
    let out = format("Zi:switch(x){case 1:break Zi}", &opts).unwrap();
    assert!(out.contains("Zi: switch"), "{out}");
}
