//! A `physure.conf` in the working directory is looked for on every unit parse that has no catalog
//! scope: it applies when it appears, while it is there, and no more once it is removed.
//!
//! Its own test binary, with one test: the working directory is process-wide, so a test that changes it
//! cannot share a binary with others that run in parallel.

use physure_core::units::Parser;
use std::fs;

#[test]
fn a_physure_conf_in_the_working_directory_applies_only_while_it_is_there() {
    let dir = std::env::temp_dir().join(format!("physure_local_conf_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let without = Parser::parse_expression("zorb");
    fs::write(dir.join("physure.conf"), "[Units]\nzorb = 5.0, L, [zorb]\n").unwrap();
    let with = Parser::parse_expression("zorb");
    let kilo_zorb = Parser::parse_expression("kzorb");
    let metre = Parser::parse_expression("m");
    fs::remove_file(dir.join("physure.conf")).unwrap();
    let after = Parser::parse_expression("zorb");

    std::env::set_current_dir(previous).unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert!(without.is_err(), "zorb is not in the embedded catalog");
    assert_eq!(with.expect("the local conf defines zorb").scale, 5.0);
    assert_eq!(kilo_zorb.expect("a prefix applies to the unit it defines").scale, 5000.0);
    assert_eq!(metre.expect("the embedded catalog is still there").scale, 1.0);
    assert!(after.is_err(), "once the file is gone, so is zorb");
}
