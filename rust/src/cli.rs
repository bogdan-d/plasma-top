//! Command-line parser and stable command contract.

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
    /// Prints one subcommand's argparse-compatible help text.
    HelpFor(&'static str),
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
    /// The top-level command name is unsupported.
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
    /// Parses the supported CLI contract.
    ///
    /// The parser accepts only documented command names and flag shapes.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] when an argument is malformed or outside the
    /// current contract. Repeated options keep their last value like argparse.
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
        if command_name != "page" && tail_values.iter().any(is_help) {
            return Ok(Self {
                command: Command::HelpFor(command_name_static(&command_name)),
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
            Self::HelpFor(_) => "help",
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
                "non-unicode arguments are not supported by the CLI parser"
            ),
            Self::UnknownCommand { command } => write!(
                formatter,
                "usage: pirostats [-h] <command> ...\npirostats: error: argument <command>: invalid choice: '{command}' (choose from 'daemon', 'render', 'probe', 'profiling', 'list-items', 'page', 'click')"
            ),
            Self::UnknownArgument { command, argument } => {
                write!(
                    formatter,
                    "{}\npirostats {command}: error: unrecognized arguments: {argument}",
                    usage_text(command)
                )
            }
            Self::MissingValue { command, flag } => {
                write!(
                    formatter,
                    "{}\npirostats {command}: error: argument {flag}: expected one argument",
                    usage_text(command)
                )
            }
            Self::InvalidValue {
                command,
                flag,
                value,
            } => {
                let choices = match (*command, *flag) {
                    ("render", "--component") => "'panel', 'tooltip', 'both'",
                    ("render", "--format") => "'text', 'html'",
                    ("render", "--layout") => "'auto', 'horizontal', 'vertical'",
                    ("render", "--page") => {
                        "'full', 'processes', 'connections', 'fastfetch', 'cpu_cores', 'graphs'"
                    }
                    ("page", "step") => "'next', 'prev'",
                    _ => "",
                };
                write!(
                    formatter,
                    "{}\npirostats {command}: error: argument {flag}: invalid choice: '{value}' (choose from {choices})",
                    usage_text(command)
                )
            }
        }
    }
}

fn usage_text(command: &str) -> &'static str {
    match command {
        "render" => {
            "usage: pirostats render [-h] [--config PATH]\n                        [--component {panel,tooltip,both}]\n                        [--format {text,html}]\n                        [--layout {auto,horizontal,vertical}]\n                        [--page {full,processes,connections,fastfetch,cpu_cores,graphs}]"
        }
        "daemon" => "usage: pirostats daemon [-h] [--config PATH]",
        "probe" => "usage: pirostats probe [-h] [--config PATH]",
        "profiling" => "usage: pirostats profiling [-h] [--config PATH]",
        "list-items" => "usage: pirostats list-items [-h]",
        "page" => "usage: pirostats page [-h] {next,prev}",
        "click" => "usage: pirostats click [-h]",
        _ => "usage: pirostats [-h] <command> ...",
    }
}

impl StdError for CliError {}

pub(crate) fn help_text() -> &'static str {
    #[cfg(any())]
    concat!(
        "pirostats\n\n",
        "KDE Plasma panel and tooltip system statistics.\n\n",
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
    );
    "usage: pirostats [-h] <command> ...\n\npositional arguments:\n  <command>\n    daemon      Production loop: renders continuously and writes the files the\n                widget reads\n    render      One-shot render of panel/tooltip, then exits\n    probe       One-shot: probe the hardware and print the raw readings (no\n                render)\n    profiling   One-shot timing report (cold/warm cache, per-section/item)\n    list-items  Lists the available items and where they can go, then exits\n    page        Switch the tooltip page (bind to the widget's mouse-wheel\n                commands)\n    click       Run the current page's click action (bind to the widget's\n                click command)\n\noptions:\n  -h, --help    show this help message and exit"
}

pub(crate) fn subcommand_help(command: &str) -> &'static str {
    match command {
        "daemon" => {
            "usage: pirostats daemon [-h] [--config PATH]\n\noptions:\n  -h, --help     show this help message and exit\n  --config PATH  Path to the TOML (default: ~/.config/pirostats/config.toml,\n                 else the shipped config)"
        }
        "probe" => {
            "usage: pirostats probe [-h] [--config PATH]\n\noptions:\n  -h, --help     show this help message and exit\n  --config PATH  Path to the TOML (default: ~/.config/pirostats/config.toml,\n                 else the shipped config)"
        }
        "profiling" => {
            "usage: pirostats profiling [-h] [--config PATH]\n\noptions:\n  -h, --help     show this help message and exit\n  --config PATH  Path to the TOML (default: ~/.config/pirostats/config.toml,\n                 else the shipped config)"
        }
        "list-items" => {
            "usage: pirostats list-items [-h]\n\noptions:\n  -h, --help  show this help message and exit"
        }
        "click" => {
            "usage: pirostats click [-h]\n\noptions:\n  -h, --help  show this help message and exit"
        }
        "page" => {
            "usage: pirostats page [-h] {next,prev}\n\npositional arguments:\n  {next,prev}  Move to the next/previous page (wraps around)\n\noptions:\n  -h, --help   show this help message and exit"
        }
        "render" => {
            "usage: pirostats render [-h] [--config PATH]\n                        [--component {panel,tooltip,both}]\n                        [--format {text,html}]\n                        [--layout {auto,horizontal,vertical}]\n                        [--page {full,processes,connections,fastfetch,cpu_cores,graphs}]\n\noptions:\n  -h, --help            show this help message and exit\n  --config PATH         Path to the TOML (default:\n                        ~/.config/pirostats/config.toml, else the shipped\n                        config)\n  --component {panel,tooltip,both}\n                        What to render (default: both)\n  --format {text,html}  text = stripped to stdout; html =\n                        /tmp/pirostats_render_* files (default: text)\n  --layout {auto,horizontal,vertical}\n                        Forces the panel orientation (horizontal = column,\n                        vertical = inline bar); auto = detection like the\n                        daemon (default)\n  --page {full,processes,connections,fastfetch,cpu_cores,graphs}\n                        Render a tooltip deep-dive page (any page, even one\n                        not in pages.order) instead of the full view; implies\n                        --component tooltip. Image pages (graphs) show only\n                        their legends in text format"
        }
        _ => "",
    }
}

fn command_name_static(command: &str) -> &'static str {
    match command {
        "daemon" => "daemon",
        "render" => "render",
        "probe" => "probe",
        "profiling" => "profiling",
        "list-items" => "list-items",
        "page" => "page",
        "click" => "click",
        _ => "",
    }
}

fn parse_config_command(
    command: &'static str,
    mut args: TailArgs,
) -> Result<ConfigCommand, CliError> {
    let mut parsed = ConfigCommand::default();

    while let Some(argument) = args.pop_front() {
        let flag = into_text(argument)?;
        match flag.as_str() {
            "--config" => {
                parsed.config = Some(args.take_value_path(command, "--config")?);
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

fn parse_render_command(mut args: TailArgs) -> Result<RenderCommand, CliError> {
    let command = "render";
    let mut parsed = RenderCommand::default();

    while let Some(argument) = args.pop_front() {
        let flag = into_text(argument)?;
        match flag.as_str() {
            "--config" => {
                parsed.config = Some(args.take_value_path(command, "--config")?);
            }
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
                parsed.component = component;
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
                parsed.format = format;
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
                parsed.layout = layout;
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
                parsed.page = Some(page);
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
    // Root Python entrypoint fast-path bypasses argparse: only exact `next`
    // advances; missing/unknown values step backward and trailing args are
    // ignored. Preserve that observable process behavior.
    let direction = if let Some(value) = args.pop_front() {
        if into_text(value)? == "next" {
            PageDirection::Next
        } else {
            PageDirection::Prev
        }
    } else {
        PageDirection::Prev
    };

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
    fn page_fast_path_treats_missing_and_unknown_as_previous() {
        for arguments in [
            &["pirostats", "page"][..],
            &["pirostats", "page", "unknown", "ignored"][..],
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
        let cli = parse(&["pirostats", "render", "--help"]);

        assert!(matches!(
            cli,
            Ok(Cli {
                command: Command::HelpFor("render")
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

    #[test]
    fn repeated_render_flags_keep_last_value_like_argparse() {
        assert_eq!(
            parse(&[
                "pirostats",
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
}
