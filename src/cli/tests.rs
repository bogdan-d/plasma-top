#![allow(clippy::expect_used)]

use super::*;

fn parse(arguments: &[&str]) -> Result<Cli, CliError> {
    Cli::parse(arguments.iter().map(OsString::from))
}

#[test]
fn defaults_to_help_without_subcommand() {
    let cli = parse(&["plasma-top"]);

    assert!(matches!(
        cli,
        Ok(Cli {
            command: Command::Help
        })
    ));
}

#[test]
fn parses_render_defaults() {
    let cli = parse(&["plasma-top", "render"]);

    assert_eq!(
        cli,
        Ok(Cli {
            command: Command::Render(RenderCommand::default()),
        })
    );
}

#[test]
fn parses_render_overrides() {
    let cli = parse(&[
        "plasma-top",
        "render",
        "--component",
        "tooltip",
        "--format",
        "html",
        "--layout",
        "vertical",
        "--page",
        "graphs",
    ]);

    assert_eq!(
        cli,
        Ok(Cli {
            command: Command::Render(RenderCommand {
                config: None,
                component: RenderComponent::Tooltip,
                format: RenderFormat::Html,
                layout: PanelLayout::Vertical,
                page: Some(RenderPage::Graphs),
            }),
        })
    );
}

#[test]
fn parses_page_direction() {
    let cli = parse(&["plasma-top", "page", "prev"]);

    assert_eq!(
        cli,
        Ok(Cli {
            command: Command::Page(PageCommand {
                direction: PageDirection::Prev,
            }),
        })
    );
}

#[test]
fn parses_strict_presentation_instance() {
    for command in ["present", "dismiss"] {
        let cli = parse(&["plasma-top", command, "42"]).expect("valid lease command");
        assert_eq!(cli.command.name(), command);
    }
}

#[test]
fn rejects_invalid_presentation_instances_and_trailing_arguments() {
    for value in ["0", "-1", "+1", "abc"] {
        assert!(matches!(
            parse(&["plasma-top", "present", value]),
            Err(CliError::InvalidInstanceId { .. })
        ));
    }
    assert!(matches!(
        parse(&["plasma-top", "dismiss", "1", "extra"]),
        Err(CliError::UnknownArgument { .. })
    ));
}

#[test]
fn page_fast_path_treats_missing_and_unknown_as_previous() {
    for arguments in [
        &["plasma-top", "page"][..],
        &["plasma-top", "page", "unknown", "ignored"][..],
    ] {
        assert!(matches!(
            parse(arguments),
            Ok(Cli {
                command: Command::Page(PageCommand {
                    direction: PageDirection::Prev
                })
            })
        ));
    }
}

#[test]
fn subcommand_help_resolves_to_its_own_help() {
    let cli = parse(&["plasma-top", "render", "--help"]);

    assert!(matches!(
        cli,
        Ok(Cli {
            command: Command::HelpFor("render")
        })
    ));
}

#[test]
fn rejects_unknown_render_choice() {
    let cli = parse(&["plasma-top", "render", "--format", "json"]);

    assert_eq!(
        cli,
        Err(CliError::InvalidValue {
            command: "render",
            flag: "--format",
            value: String::from("json"),
        })
    );
}

#[test]
fn rejects_out_of_scope_list_items_flags() {
    let cli = parse(&["plasma-top", "list-items", "--config", "config.toml"]);

    assert_eq!(
        cli,
        Err(CliError::UnknownArgument {
            command: "list-items",
            argument: String::from("--config"),
        })
    );
}

#[test]
fn repeated_render_flags_keep_last_value_like_argparse() {
    assert_eq!(
        parse(&[
            "plasma-top",
            "render",
            "--component",
            "both",
            "--component",
            "panel",
        ]),
        Ok(Cli {
            command: Command::Render(RenderCommand {
                component: RenderComponent::Panel,
                ..RenderCommand::default()
            }),
        })
    );
}

#[test]
fn timed_profiling_defaults_to_main_and_accepts_fractional_seconds() {
    assert_eq!(
        parse(&["plasma-top", "profiling", "--duration", "0.25"]),
        Ok(Cli {
            command: Command::Profiling(ProfilingCommand {
                config: None,
                duration: Some(Duration::from_millis(250)),
                scenario: Some(String::from("main")),
                stimuli: false,
            }),
        })
    );
}

#[test]
fn profiling_rejects_invalid_duration_and_scenario_without_duration() {
    for value in ["0", "-1", "NaN", "inf", "text", "0.0000000001"] {
        assert!(matches!(
            parse(&["plasma-top", "profiling", "--duration", value]),
            Err(CliError::InvalidDuration { .. })
        ));
    }
    assert_eq!(
        parse(&["plasma-top", "profiling", "--scenario", "hidden"]),
        Err(CliError::ScenarioRequiresDuration)
    );
    assert_eq!(
        parse(&["plasma-top", "profiling", "--stimuli"]),
        Err(CliError::StimuliRequireDuration)
    );
}

#[test]
fn profiling_stimuli_are_an_explicit_timed_opt_in() {
    assert_eq!(
        parse(&[
            "plasma-top",
            "profiling",
            "--duration",
            "1",
            "--scenario",
            "hidden",
            "--stimuli",
        ]),
        Ok(Cli {
            command: Command::Profiling(ProfilingCommand {
                duration: Some(Duration::from_secs(1)),
                scenario: Some(String::from("hidden")),
                stimuli: true,
                ..ProfilingCommand::default()
            }),
        })
    );
}

#[test]
fn profiling_rejects_legacy_full_scenario_alias() {
    assert_eq!(
        parse(&[
            "plasma-top",
            "profiling",
            "--duration",
            "1",
            "--scenario",
            "full",
        ]),
        Err(CliError::InvalidProfilingScenario {
            value: String::from("full"),
        })
    );
}

#[test]
fn one_shot_profiling_contract_is_unchanged() {
    assert_eq!(
        parse(&["plasma-top", "profiling", "--config", "custom.toml"]),
        Ok(Cli {
            command: Command::Profiling(ProfilingCommand {
                config: Some(PathBuf::from("custom.toml")),
                ..ProfilingCommand::default()
            }),
        })
    );
}
