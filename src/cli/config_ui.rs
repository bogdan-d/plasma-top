//! Command grammar for widget-owned daemon configuration.

use super::{CliError, ConfigUiCommand, TailArgs, into_text};

pub(super) fn parse(mut args: TailArgs) -> Result<ConfigUiCommand, CliError> {
    let action = args
        .pop_front()
        .ok_or(CliError::MissingValue {
            command: "config",
            flag: "action",
        })
        .and_then(into_text)?;
    let command = match action.as_str() {
        "init" => ConfigUiCommand::Init,
        "show" => ConfigUiCommand::Show,
        "apply" => {
            let mut next = |flag| args.take_value("config", flag).and_then(into_text);
            ConfigUiCommand::Apply {
                poll: next("POLL")?,
                history: next("HISTORY")?,
                pages: next("PAGES")?,
                notifications: next("NOTIFICATIONS")?,
            }
        }
        "tooltip" => {
            let operation = args
                .take_value("config", "tooltip action")
                .and_then(into_text)?;
            match operation.as_str() {
                "show" => ConfigUiCommand::TooltipShow,
                "apply" => ConfigUiCommand::TooltipApply {
                    payload: args.take_value("config", "JSON").and_then(into_text)?,
                },
                _ => {
                    return Err(CliError::InvalidValue {
                        command: "config",
                        flag: "tooltip action",
                        value: operation,
                    });
                }
            }
        }
        "graphs" => {
            let operation = args
                .take_value("config", "graphs action")
                .and_then(into_text)?;
            match operation.as_str() {
                "show" => ConfigUiCommand::GraphsShow,
                "apply" => ConfigUiCommand::GraphsApply {
                    payload: args.take_value("config", "JSON").and_then(into_text)?,
                },
                _ => {
                    return Err(CliError::InvalidValue {
                        command: "config",
                        flag: "graphs action",
                        value: operation,
                    });
                }
            }
        }
        _ => {
            return Err(CliError::InvalidValue {
                command: "config",
                flag: "action",
                value: action,
            });
        }
    };
    if let Some(argument) = args.pop_front() {
        return Err(CliError::UnknownArgument {
            command: "config",
            argument: into_text(argument)?,
        });
    }
    Ok(command)
}
