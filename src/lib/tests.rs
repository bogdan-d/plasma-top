use super::*;

#[test]
fn run_accepts_help_without_runtime() {
    let args = [OsString::from("plasma-top"), OsString::from("--help")];

    let result = run(args);

    assert!(result.is_ok());
}

#[test]
fn help_text_omits_internal_migration_terms() {
    assert!(!cli::help_text().contains("scaffold"));
}
