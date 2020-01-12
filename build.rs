extern crate vergen;
use std::process::Command;
use vergen::{generate_cargo_keys, ConstantsFlags};

fn main() {
    let output = Command::new("git")
        .arg("status")
        .arg("--short")
        .output()
        .expect("Could not determine if workspace is dirty");

    if !output.status.success() {
        panic!(
            "Command 'git status --short' executed with failing error code, {}",
            output.status
        );
    }
    println!(
        "cargo:rustc-env=BUILD_GIT_WORKSPACE_IS_DIRTY={}",
        output.stdout.len() > 0 || output.stderr.len() > 0
    );
    // Generate the 'cargo:' key output
    generate_cargo_keys(ConstantsFlags::all()).expect("Unable to generate the cargo keys!");
}
