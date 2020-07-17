use std::{
    env,
    io::Write,
    process::{Command, Stdio},
    str,
};

fn run_koto_repl_test(inputs: &[&str], expected_outputs: &[&str]) {
    let mut process = Command::new(env!("CARGO_BIN_EXE_koto"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute child");

    let stdin = process.stdin.as_mut().expect("failed to get stdin");

    for input in inputs.iter() {
        stdin
            .write(input.as_bytes())
            .expect("Failed to write to stdin");
        stdin
            .write("\n".as_bytes())
            .expect("Failed to write to stdin");
    }

    let output = process.wait_with_output().expect("Failed to get output");
    let stdout = String::from_utf8(output.stdout).expect("Failed to get output");
    let mut output_lines = stdout.lines().skip_while(|line| line != &"» ");

    for expected in expected_outputs.iter() {
        output_lines.next(); // prompt (empty line in test)
        assert_eq!(output_lines.next().expect("Missing output"), *expected);
    }
}

#[test]
fn basic_arithmetic() {
    run_koto_repl_test(&["a = 2", "a + a"], &["2", "4"]);
}

#[test]
fn import_assert() {
    run_koto_repl_test(&["import test.assert", "assert true"], &["External Function", "()"]);
}
