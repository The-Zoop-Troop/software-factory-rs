//! `harden_process` on this kernel: before it, a child reads its parent's `/proc` environ (where
//! a role keeps the ledger password); after it, the child cannot. Its own test binary, because
//! the change applies to the whole process.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::process::Command;

fn child_reads_parent_environ() -> bool {
    Command::new("/bin/sh")
        .args(["-c", "cat /proc/$PPID/environ > /dev/null"])
        .status()
        .unwrap()
        .success()
}

#[test]
fn a_hardened_role_hides_its_environment_from_its_children() {
    assert!(
        child_reads_parent_environ(),
        "control: readable before hardening"
    );
    infra::harden_process().unwrap();
    assert!(
        !child_reads_parent_environ(),
        "a child read the hardened parent's environ"
    );
}
