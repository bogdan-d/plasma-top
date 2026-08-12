//! Command-line parser and stable command contract.

use std::collections::VecDeque;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::time::Duration;

use crate::runtime::presentation::InstanceId;

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
    Profiling(ProfilingCommand),
    /// Future list-items diagnostic entry point.
    ListItems,
    /// Future page-step entry point.
    Page(PageCommand),
    /// Refreshes one applet instance's presentation lease.
    Present(PresentationCommand),
    /// Removes one applet instance's presentation lease.
    Dismiss(PresentationCommand),
    /// Future click entry point.
    Click,
}

/// A command that accepts an optional config path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigCommand {
    /// Optional path to a specific TOML configuration file.
    pub config: Option<PathBuf>,
}

/// Parsed arguments for one-shot or timed scheduler profiling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfilingCommand {
    /// Optional path to a specific TOML configuration file.
    pub config: Option<PathBuf>,
    /// Timed scheduler run length. Absence preserves one-shot profiling.
    pub duration: Option<Duration>,
    /// Timed presentation scenario (`hidden`, `main`, or a configured page id).
    pub scenario: Option<String>,
    /// Opts a timed run into bounded page, presentation, and config stimuli.
    pub stimuli: bool,
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

/// Parsed presentation lease command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationCommand {
    /// Strict positive numeric Plasma applet instance identifier.
    pub instance: InstanceId,
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
    /// A presentation instance id was not a strict positive decimal integer.
    InvalidInstanceId {
        /// The owning command.
        command: &'static str,
        /// The rejected value.
        value: String,
    },
    /// A profiling duration was not finite and strictly positive.
    InvalidDuration {
        /// The rejected value.
        value: String,
    },
    /// A profiling scenario was supplied without timed mode.
    ScenarioRequiresDuration,
    /// Profiling stimuli were requested without timed mode.
    StimuliRequireDuration,
    /// The legacy `full` alias is not a documented profiling scenario.
    InvalidProfilingScenario {
        /// The rejected scenario.
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
            "profiling" => Command::Profiling(parse_profiling_command(tail)?),
            "list-items" => parse_list_items_command(tail)?,
            "page" => Command::Page(parse_page_command(tail)?),
            "present" => Command::Present(parse_presentation_command("present", tail)?),
            "dismiss" => Command::Dismiss(parse_presentation_command("dismiss", tail)?),
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
            Self::Present(_) => "present",
            Self::Dismiss(_) => "dismiss",
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
                "usage: plasma-top [-h] <command> ...\nplasma-top: error: argument <command>: invalid choice: '{command}' (choose from 'daemon', 'render', 'probe', 'profiling', 'list-items', 'page', 'click', 'present', 'dismiss')"
            ),
            Self::UnknownArgument { command, argument } => {
                write!(
                    formatter,
                    "{}\nplasma-top {command}: error: unrecognized arguments: {argument}",
                    usage_text(command)
                )
            }
            Self::MissingValue { command, flag } => {
                write!(
                    formatter,
                    "{}\nplasma-top {command}: error: argument {flag}: expected one argument",
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
                    "{}\nplasma-top {command}: error: argument {flag}: invalid choice: '{value}' (choose from {choices})",
                    usage_text(command)
                )
            }
            Self::InvalidInstanceId { command, value } => write!(
                formatter,
                "{}\nplasma-top {command}: error: instance-id must be a positive decimal integer: '{value}'",
                usage_text(command)
            ),
            Self::InvalidDuration { value } => write!(
                formatter,
                "{}\nplasma-top profiling: error: argument --duration: must be a finite positive number of seconds: '{value}'",
                usage_text("profiling")
            ),
            Self::ScenarioRequiresDuration => write!(
                formatter,
                "{}\nplasma-top profiling: error: argument --scenario requires --duration",
                usage_text("profiling")
            ),
            Self::StimuliRequireDuration => write!(
                formatter,
                "{}\nplasma-top profiling: error: argument --stimuli requires --duration",
                usage_text("profiling")
            ),
            Self::InvalidProfilingScenario { value } => write!(
                formatter,
                "{}\nplasma-top profiling: error: scenario must be 'hidden', 'main', or a configured page id; unsupported alias: '{value}'",
                usage_text("profiling")
            ),
        }
    }
}

fn usage_text(command: &str) -> &'static str {
    match command {
        "render" => {
            "usage: plasma-top render [-h] [--config PATH]\n                        [--component {panel,tooltip,both}]\n                        [--format {text,html}]\n                        [--layout {auto,horizontal,vertical}]\n                        [--page {full,processes,connections,fastfetch,cpu_cores,graphs}]"
        }
        "daemon" => "usage: plasma-top daemon [-h] [--config PATH]",
        "probe" => "usage: plasma-top probe [-h] [--config PATH]",
        "profiling" => {
            "usage: plasma-top profiling [-h] [--config PATH] [--duration SECONDS] [--scenario hidden|main|PAGE] [--stimuli]"
        }
        "list-items" => "usage: plasma-top list-items [-h]",
        "page" => "usage: plasma-top page [-h] {next,prev}",
        "present" => "usage: plasma-top present [-h] instance-id",
        "dismiss" => "usage: plasma-top dismiss [-h] instance-id",
        "click" => "usage: plasma-top click [-h]",
        _ => "usage: plasma-top [-h] <command> ...",
    }
}

impl StdError for CliError {}

pub(crate) fn help_text() -> &'static str {
    #[cfg(any())]
    concat!(
        "plasma-top\n\n",
        "KDE Plasma panel and tooltip system statistics.\n\n",
        "USAGE:\n",
        "    plasma-top <command> [options]\n\n",
        "COMMANDS:\n",
        "    daemon [--config PATH]\n",
        "    render [--config PATH] [--component panel|tooltip|both] [--format text|html] \\\n",
        "           [--layout auto|horizontal|vertical] [--page full|processes|cpu_cores|connections|fastfetch|graphs]\n",
        "    probe [--config PATH]\n",
        "    profiling [--config PATH] [--duration SECONDS] [--scenario hidden|main|PAGE] [--stimuli]\n",
        "    list-items\n",
        "    page <next|prev>\n",
        "    click\n",
        "    --help\n",
        "    --version\n",
    );
    "usage: plasma-top [-h] <command> ...\n\npositional arguments:\n  <command>\n    daemon      Production loop: renders continuously and writes the files the\n                widget reads\n    render      One-shot render of panel/tooltip, then exits\n    probe       One-shot: probe the hardware and print the raw readings (no\n                render)\n    profiling   One-shot timing report (cold/warm cache, per-section/item)\n    list-items  Lists the available items and where they can go, then exits\n    page        Switch the tooltip page (bind to the widget's mouse-wheel\n                commands)\n    click       Run the current page's click action (bind to the widget's\n                click command)\n\noptions:\n  -h, --help    show this help message and exit"
}

pub(crate) fn subcommand_help(command: &str) -> &'static str {
    match command {
        "daemon" => {
            "usage: plasma-top daemon [-h] [--config PATH]\n\noptions:\n  -h, --help     show this help message and exit\n  --config PATH  Path to the TOML (default: ~/.config/plasma-top/config.toml,\n                 else the shipped config)"
        }
        "probe" => {
            "usage: plasma-top probe [-h] [--config PATH]\n\noptions:\n  -h, --help     show this help message and exit\n  --config PATH  Path to the TOML (default: ~/.config/plasma-top/config.toml,\n                 else the shipped config)"
        }
        "profiling" => {
            "usage: plasma-top profiling [-h] [--config PATH] [--duration SECONDS] [--scenario hidden|main|PAGE] [--stimuli]\n\noptions:\n  -h, --help          show this help message and exit\n  --config PATH       Path to the TOML (default: ~/.config/plasma-top/config.toml,\n                      else the shipped config)\n  --duration SECONDS  Run the real async scheduler for a finite positive duration\n  --scenario VALUE    hidden, main (default with --duration), or a configured page id\n  --stimuli           Opt into serial page/presentation/config stimulus measurement"
        }
        "list-items" => {
            "usage: plasma-top list-items [-h]\n\noptions:\n  -h, --help  show this help message and exit"
        }
        "click" => {
            "usage: plasma-top click [-h]\n\noptions:\n  -h, --help  show this help message and exit"
        }
        "page" => {
            "usage: plasma-top page [-h] {next,prev}\n\npositional arguments:\n  {next,prev}  Move to the next/previous page (wraps around)\n\noptions:\n  -h, --help   show this help message and exit"
        }
        "present" => "usage: plasma-top present [-h] instance-id",
        "dismiss" => "usage: plasma-top dismiss [-h] instance-id",
        "render" => {
            "usage: plasma-top render [-h] [--config PATH]\n                        [--component {panel,tooltip,both}]\n                        [--format {text,html}]\n                        [--layout {auto,horizontal,vertical}]\n                        [--page {full,processes,connections,fastfetch,cpu_cores,graphs}]\n\noptions:\n  -h, --help            show this help message and exit\n  --config PATH         Path to the TOML (default:\n                        ~/.config/plasma-top/config.toml, else the shipped\n                        config)\n  --component {panel,tooltip,both}\n                        What to render (default: both)\n  --format {text,html}  text = stripped to stdout; html =\n                        /tmp/plasma-top_render_* files (default: text)\n  --layout {auto,horizontal,vertical}\n                        Forces the panel orientation (horizontal = column,\n                        vertical = inline bar); auto = detection like the\n                        daemon (default)\n  --page {full,processes,connections,fastfetch,cpu_cores,graphs}\n                        Render a tooltip deep-dive page (any page, even one\n                        not in pages.order) instead of the full view; implies\n                        --component tooltip. Image pages (graphs) show only\n                        their legends in text format"
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
        "present" => "present",
        "dismiss" => "dismiss",
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

fn parse_profiling_command(mut args: TailArgs) -> Result<ProfilingCommand, CliError> {
    let command = "profiling";
    let mut parsed = ProfilingCommand::default();
    while let Some(argument) = args.pop_front() {
        let flag = into_text(argument)?;
        match flag.as_str() {
            "--config" => parsed.config = Some(args.take_value_path(command, "--config")?),
            "--duration" => {
                let value = into_text(args.take_value(command, "--duration")?)?;
                let seconds = value
                    .parse::<f64>()
                    .ok()
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                    .ok_or_else(|| CliError::InvalidDuration {
                        value: value.clone(),
                    })?;
                parsed.duration = Some(
                    Duration::try_from_secs_f64(seconds)
                        .ok()
                        .filter(|duration| !duration.is_zero())
                        .ok_or(CliError::InvalidDuration { value })?,
                );
            }
            "--scenario" => {
                let value = into_text(args.take_value(command, "--scenario")?)?;
                if value == "full" {
                    return Err(CliError::InvalidProfilingScenario { value });
                }
                parsed.scenario = Some(value);
            }
            "--stimuli" => parsed.stimuli = true,
            _ => {
                return Err(CliError::UnknownArgument {
                    command,
                    argument: flag,
                });
            }
        }
    }
    if parsed.duration.is_none() && parsed.scenario.is_some() {
        return Err(CliError::ScenarioRequiresDuration);
    }
    if parsed.duration.is_none() && parsed.stimuli {
        return Err(CliError::StimuliRequireDuration);
    }
    if parsed.duration.is_some() && parsed.scenario.is_none() {
        parsed.scenario = Some(String::from("main"));
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

fn parse_presentation_command(
    command: &'static str,
    mut args: TailArgs,
) -> Result<PresentationCommand, CliError> {
    let value = into_text(args.take_value(command, "instance-id")?)?;
    let instance = value.parse().map_err(|_| CliError::InvalidInstanceId {
        command,
        value: value.clone(),
    })?;
    if let Some(argument) = args.pop_front() {
        return Err(CliError::UnknownArgument {
            command,
            argument: into_text(argument)?,
        });
    }
    Ok(PresentationCommand { instance })
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
mod tests;
