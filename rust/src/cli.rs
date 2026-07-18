//! Thin CLI contract for the Rust scaffold.

use std::collections::VecDeque;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

/// Parsed top-level CLI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    /// The requested command.
    pub command: Command,
}

/// Supported top-level commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Prints the top-level help text.
    Help,
    /// Prints the crate version.
    Version,
    /// Future daemon entry point.
    Daemon(ConfigCommand),
    /// Future one-shot render entry point.
    Render(RenderCommand),
    /// Future raw probe entry point.
    Probe(ConfigCommand),
    /// Future profiling entry point.
    Profiling(ConfigCommand),
    /// Future list-items diagnostic entry point.
    ListItems,
    /// Future page-step entry point.
    Page(PageCommand),
    /// Future click entry point.
    Click,
}

/// A command that accepts an optional config path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigCommand {
    /// Optional path to a specific TOML configuration file.
    pub config: Option<PathBuf>,
}

/// Parsed arguments for the future `render` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCommand {
    /// Optional path to a specific TOML configuration file.
    pub config: Option<PathBuf>,
    /// Which rendered surface to emit.
    pub component: RenderComponent,
    /// Which output representation to emit.
    pub format: RenderFormat,
    /// Which panel layout to use.
    pub layout: PanelLayout,
    /// Optional tooltip page override.
    pub page: Option<RenderPage>,
}

impl Default for RenderCommand {
    fn default() -> Self {
        Self {
            config: None,
            component: RenderComponent::Both,
            format: RenderFormat::Text,
            layout: PanelLayout::Auto,
            page: None,
        }
    }
}

/// Render surface selection for the future `render` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderComponent {
    /// Only the panel output.
    Panel,
    /// Only the tooltip output.
    Tooltip,
    /// Both panel and tooltip outputs.
    Both,
}

/// Output representation for the future `render` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    /// Plain text written to stdout.
    Text,
    /// HTML written to diagnostic files.
    Html,
}

/// Panel layout override for the future `render` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelLayout {
    /// Use runtime auto-detection.
    Auto,
    /// Force horizontal panel semantics.
    Horizontal,
    /// Force vertical panel semantics.
    Vertical,
}

/// Tooltip page selection for the future `render` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPage {
    /// The full stats tooltip view.
    Full,
    /// The processes deep-dive page.
    Processes,
    /// The per-core CPU deep-dive page.
    CpuCores,
    /// The socket/listener deep-dive page.
    Connections,
    /// The fastfetch deep-dive page.
    Fastfetch,
    /// The graphs deep-dive page.
    Graphs,
}

/// Parsed arguments for the future `page` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCommand {
    /// Which direction to step the page counter.
    pub direction: PageDirection,
}

/// Page stepping direction for the future `page` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDirection {
    /// Move forward one page with wrap-around.
    Next,
    /// Move backward one page with wrap-around.
    Prev,
}

/// CLI parsing errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The command line contained a non-Unicode token where a textual flag or subcommand was expected.
    NonUnicodeArgument,
    /// The top-level command name is not part of the scaffold contract.
    UnknownCommand {
        /// The unrecognized command text.
        command: String,
    },
    /// A flag was not accepted for the current command.
    UnknownArgument {
        /// The owning command.
        command: &'static str,
        /// The unrecognized flag or positional argument.
        argument: String,
    },
    /// A required flag value was missing.
    MissingValue {
        /// The owning command.
        command: &'static str,
        /// The flag missing its following value.
        flag: &'static str,
    },
    /// A repeated flag attempted to set the same field twice.
    DuplicateArgument {
        /// The owning command.
        command: &'static str,
        /// The repeated flag.
        flag: &'static str,
    },
    /// A flag value was syntactically present but not one of the accepted choices.
    InvalidValue {
        /// The owning command.
        command: &'static str,
        /// The flag whose value was invalid.
        flag: &'static str,
        /// The rejected value.
        value: String,
    },
}

impl Cli {
    /// Parses the current scaffold CLI contract.
    ///
    /// The parser intentionally accepts only the current command names and flag
    /// shapes needed to freeze the contract for later implementation slices.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] when an argument is malformed, duplicated, or
    /// outside the current contract.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        let mut args = args.into_iter();
        let _program = args.next();

        let Some(command) = args.next() else {
            return Ok(Self {
                command: Command::Help,
            });
        };

        if is_help(&command) {
            return Ok(Self {
                command: Command::Help,
            });
        }

        if is_version(&command) {
            return Ok(Self {
                command: Command::Version,
            });
        }

        let command_name = into_text(command)?;
        let tail_values: Vec<OsString> = args.collect();
        if tail_values.iter().any(is_help) {
            return Ok(Self {
                command: Command::Help,
            });
        }
        let tail = TailArgs::new(tail_values);
        let command = match command_name.as_str() {
            "daemon" => Command::Daemon(parse_config_command("daemon", tail)?),
            "render" => Command::Render(parse_render_command(tail)?),
            "probe" => Command::Probe(parse_config_command("probe", tail)?),
            "profiling" => Command::Profiling(parse_config_command("profiling", tail)?),
            "list-items" => parse_list_items_command(tail)?,
            "page" => Command::Page(parse_page_command(tail)?),
            "click" => parse_click_command(tail)?,
            _ => {
                return Err(CliError::UnknownCommand {
                    command: command_name,
                });
            }
        };

        Ok(Self { command })
    }
}

impl Command {
    /// Returns the stable top-level command name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Version => "version",
            Self::Daemon(_) => "daemon",
            Self::Render(_) => "render",
            Self::Probe(_) => "probe",
            Self::Profiling(_) => "profiling",
            Self::ListItems => "list-items",
            Self::Page(_) => "page",
            Self::Click => "click",
        }
    }
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUnicodeArgument => write!(
                formatter,
                "non-unicode arguments are not supported by the scaffold parser"
            ),
            Self::UnknownCommand { command } => write!(formatter, "unknown command: {command}"),
            Self::UnknownArgument { command, argument } => {
                write!(formatter, "unknown argument for `{command}`: {argument}")
            }
            Self::MissingValue { command, flag } => {
                write!(formatter, "missing value for `{flag}` on `{command}`")
            }
            Self::DuplicateArgument { command, flag } => {
                write!(formatter, "duplicate argument `{flag}` on `{command}`")
            }
            Self::InvalidValue {
                command,
                flag,
                value,
            } => write!(
                formatter,
                "invalid value for `{flag}` on `{command}`: {value}"
            ),
        }
    }
}

impl StdError for CliError {}

pub(crate) fn help_text() -> &'static str {
    concat!(
        "pirostats\n\n",
        "Phase 1 Rust scaffold: commands are parsed but runtime behavior is intentionally deferred.\n\n",
        "USAGE:\n",
        "    pirostats <command> [options]\n\n",
        "COMMANDS:\n",
        "    daemon [--config PATH]\n",
        "    render [--config PATH] [--component panel|tooltip|both] [--format text|html] \\\n",
        "           [--layout auto|horizontal|vertical] [--page full|processes|cpu_cores|connections|fastfetch|graphs]\n",
        "    probe [--config PATH]\n",
        "    profiling [--config PATH]\n",
        "    list-items\n",
        "    page <next|prev>\n",
        "    click\n",
        "    --help\n",
        "    --version\n",
    )
}

fn parse_config_command(
    command: &'static str,
    mut args: TailArgs,
) -> Result<ConfigCommand, CliError> {
    let mut parsed = ConfigCommand::default();

    while let Some(argument) = args.pop_front() {
        let flag = into_text(argument)?;
        match flag.as_str() {
            "--config" => set_once_path(
                command,
                "--config",
                &mut parsed.config,
                args.take_value_path(command, "--config")?,
            )?,
            _ => {
                return Err(CliError::UnknownArgument {
                    command,
                    argument: flag,
                });
            }
        }
    }

    Ok(parsed)
}

fn parse_render_command(mut args: TailArgs) -> Result<RenderCommand, CliError> {
    let command = "render";
    let mut parsed = RenderCommand::default();

    while let Some(argument) = args.pop_front() {
        let flag = into_text(argument)?;
        match flag.as_str() {
            "--config" => set_once_path(
                command,
                "--config",
                &mut parsed.config,
                args.take_value_path(command, "--config")?,
            )?,
            "--component" => {
                let value = into_text(args.take_value(command, "--component")?)?;
                let component = match value.as_str() {
                    "panel" => RenderComponent::Panel,
                    "tooltip" => RenderComponent::Tooltip,
                    "both" => RenderComponent::Both,
                    _ => {
                        return Err(CliError::InvalidValue {
                            command,
                            flag: "--component",
                            value,
                        });
                    }
                };
                set_once_copy(
                    command,
                    "--component",
                    &mut parsed.component,
                    component,
                    RenderComponent::Both,
                )?;
            }
            "--format" => {
                let value = into_text(args.take_value(command, "--format")?)?;
                let format = match value.as_str() {
                    "text" => RenderFormat::Text,
                    "html" => RenderFormat::Html,
                    _ => {
                        return Err(CliError::InvalidValue {
                            command,
                            flag: "--format",
                            value,
                        });
                    }
                };
                set_once_copy(
                    command,
                    "--format",
                    &mut parsed.format,
                    format,
                    RenderFormat::Text,
                )?;
            }
            "--layout" => {
                let value = into_text(args.take_value(command, "--layout")?)?;
                let layout = match value.as_str() {
                    "auto" => PanelLayout::Auto,
                    "horizontal" => PanelLayout::Horizontal,
                    "vertical" => PanelLayout::Vertical,
                    _ => {
                        return Err(CliError::InvalidValue {
                            command,
                            flag: "--layout",
                            value,
                        });
                    }
                };
                set_once_copy(
                    command,
                    "--layout",
                    &mut parsed.layout,
                    layout,
                    PanelLayout::Auto,
                )?;
            }
            "--page" => {
                let value = into_text(args.take_value(command, "--page")?)?;
                let page = match value.as_str() {
                    "full" => RenderPage::Full,
                    "processes" => RenderPage::Processes,
                    "cpu_cores" => RenderPage::CpuCores,
                    "connections" => RenderPage::Connections,
                    "fastfetch" => RenderPage::Fastfetch,
                    "graphs" => RenderPage::Graphs,
                    _ => {
                        return Err(CliError::InvalidValue {
                            command,
                            flag: "--page",
                            value,
                        });
                    }
                };
                set_once_option(command, "--page", &mut parsed.page, page)?;
            }
            _ => {
                return Err(CliError::UnknownArgument {
                    command,
                    argument: flag,
                });
            }
        }
    }

    Ok(parsed)
}

fn parse_list_items_command(mut args: TailArgs) -> Result<Command, CliError> {
    if let Some(argument) = args.pop_front() {
        return Err(CliError::UnknownArgument {
            command: "list-items",
            argument: into_text(argument)?,
        });
    }

    Ok(Command::ListItems)
}

fn parse_page_command(mut args: TailArgs) -> Result<PageCommand, CliError> {
    let value = into_text(args.take_value("page", "step")?)?;
    let direction = match value.as_str() {
        "next" => PageDirection::Next,
        "prev" => PageDirection::Prev,
        _ => {
            return Err(CliError::InvalidValue {
                command: "page",
                flag: "step",
                value,
            });
        }
    };

    if let Some(argument) = args.pop_front() {
        return Err(CliError::UnknownArgument {
            command: "page",
            argument: into_text(argument)?,
        });
    }

    Ok(PageCommand { direction })
}

fn parse_click_command(mut args: TailArgs) -> Result<Command, CliError> {
    if let Some(argument) = args.pop_front() {
        return Err(CliError::UnknownArgument {
            command: "click",
            argument: into_text(argument)?,
        });
    }

    Ok(Command::Click)
}

fn set_once_path(
    command: &'static str,
    flag: &'static str,
    slot: &mut Option<PathBuf>,
    value: PathBuf,
) -> Result<(), CliError> {
    if slot.is_some() {
        return Err(CliError::DuplicateArgument { command, flag });
    }

    *slot = Some(value);
    Ok(())
}

fn set_once_option<T>(
    command: &'static str,
    flag: &'static str,
    slot: &mut Option<T>,
    value: T,
) -> Result<(), CliError> {
    if slot.is_some() {
        return Err(CliError::DuplicateArgument { command, flag });
    }

    *slot = Some(value);
    Ok(())
}

fn set_once_copy<T: Copy + PartialEq>(
    command: &'static str,
    flag: &'static str,
    slot: &mut T,
    value: T,
    default: T,
) -> Result<(), CliError> {
    if *slot != default {
        return Err(CliError::DuplicateArgument { command, flag });
    }

    *slot = value;
    Ok(())
}

fn is_help(argument: &OsString) -> bool {
    matches!(argument.to_str(), Some("-h" | "--help"))
}

fn is_version(argument: &OsString) -> bool {
    matches!(argument.to_str(), Some("-V" | "--version"))
}

fn into_text(argument: OsString) -> Result<String, CliError> {
    argument
        .into_string()
        .map_err(|_| CliError::NonUnicodeArgument)
}

#[derive(Debug, Clone)]
struct TailArgs {
    values: VecDeque<OsString>,
}

impl TailArgs {
    fn new(values: Vec<OsString>) -> Self {
        Self {
            values: values.into(),
        }
    }

    fn pop_front(&mut self) -> Option<OsString> {
        self.values.pop_front()
    }

    fn take_value(
        &mut self,
        command: &'static str,
        flag: &'static str,
    ) -> Result<OsString, CliError> {
        self.values
            .pop_front()
            .ok_or(CliError::MissingValue { command, flag })
    }

    fn take_value_path(
        &mut self,
        command: &'static str,
        flag: &'static str,
    ) -> Result<PathBuf, CliError> {
        Ok(PathBuf::from(self.take_value(command, flag)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Cli, CliError> {
        Cli::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn defaults_to_help_without_subcommand() {
        let cli = parse(&["pirostats"]);

        assert!(matches!(
            cli,
            Ok(Cli {
                command: Command::Help
            })
        ));
    }

    #[test]
    fn parses_render_defaults() {
        let cli = parse(&["pirostats", "render"]);

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
            "pirostats",
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
        let cli = parse(&["pirostats", "page", "prev"]);

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
    fn subcommand_help_resolves_to_top_level_help() {
        let cli = parse(&["pirostats", "render", "--help"]);

        assert!(matches!(
            cli,
            Ok(Cli {
                command: Command::Help
            })
        ));
    }

    #[test]
    fn rejects_unknown_render_choice() {
        let cli = parse(&["pirostats", "render", "--format", "json"]);

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
        let cli = parse(&["pirostats", "list-items", "--config", "config.toml"]);

        assert_eq!(
            cli,
            Err(CliError::UnknownArgument {
                command: "list-items",
                argument: String::from("--config"),
            })
        );
    }
}
