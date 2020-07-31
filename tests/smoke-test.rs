use assert_cmd::Command;

#[test]
fn smoke_test() {
    let fake_pulses = r#"
    [
     {
         "filePath": "src/bin/main.rs",
         "eventType": "typing",
         "eventDate": 1595868513238,
         "editor": "emacs 😭"
     }
    ]
    "#;

    Command::cargo_bin("activity-insights")
        .unwrap()
        .write_stdin(fake_pulses)
        .assert()
        .success();
}
