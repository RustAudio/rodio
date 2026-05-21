extern crate libtest_mimic;

use libtest_mimic::{Arguments, Failed, Trial};
use std::{
    fs::read_dir,
    process::{Command, ExitCode, Stdio},
};

fn main() -> ExitCode {
    let args = Arguments::from_args();

    let mut tests = Vec::new();

    let host_tuple = {
        let output = Command::new("rustc")
            .args(["--print", "host-tuple"])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };

    // collect test cases
    for file in read_dir("tests/rtsan_tests/src/bin").unwrap() {
        let file = file.unwrap();

        assert!(file.metadata().unwrap().is_file());

        let name = file
            .file_name()
            .to_str()
            .unwrap()
            .strip_suffix(".rs")
            .unwrap()
            .to_owned();
        let host_tuple = host_tuple.clone();

        let test = Trial::test(name.clone(), move || {
            let process = Command::new("cargo")
                .args([
                    "+nightly",
                    "run",
                    "-p",
                    "rtsan_tests",
                    "--bin",
                    &name,
                    // this puts cargo in "cross compilation mode", so it doesn't try to compile the build scripts with sanitizers,
                    "--target",
                    &host_tuple,
                ])
                .env("RUSTFLAGS", "-Zsanitizer=realtime")
                .stderr(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let output = process.wait_with_output().unwrap();
            if output.status.success() {
                Ok(())
            } else {
                Err(Failed::from(
                    String::from("realtime violation detected. Output: \n")
                        + &String::from_utf8_lossy(&output.stderr),
                ))
            }
        });
        tests.push(test);
    }

    libtest_mimic::run(&args, tests).exit_code()
}
