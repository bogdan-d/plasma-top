//! Small CLI bridge between the widget settings page and the user TOML file.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::str::FromStr;

use nix::fcntl::{Flock, FlockArg};
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::cli::ConfigUiCommand;
use crate::config::{
    DisplayConfig, NotificationConfig, PagesConfig,
    assets::{shipped_config, xdg_dir},
};
use crate::error::{Error, Result};

const PAGES: [&str; 5] = [
    "graphs",
    "processes",
    "cpu_cores",
    "connections",
    "fastfetch",
];
const NOTIFICATIONS: [&str; 11] = [
    "disk_usage",
    "disk_smart",
    "cpu_temp",
    "hd_temp",
    "gpu_nvidia_temp",
    "gpu_amd_temp",
    "battery_sys",
    "battery_mouse",
    "battery_kbd",
    "load_avg",
    "server_check",
];

#[derive(Default, Deserialize)]
#[serde(default)]
struct UiSections {
    display: DisplayConfig,
    pages: PagesConfig,
    notifications: NotificationConfig,
}

/// Runs one user-config action requested by the installer or widget page.
///
/// # Errors
///
/// Returns a contextual error if the config cannot be read, parsed, or saved.
pub fn run(command: ConfigUiCommand) -> Result<()> {
    let path = xdg_dir().join("config.toml");
    match command {
        ConfigUiCommand::Init => init(&path, &shipped_config()),
        ConfigUiCommand::Show => {
            init(&path, &shipped_config())?;
            print!("{}", show(&path)?);
            Ok(())
        }
        ConfigUiCommand::Apply {
            poll,
            history,
            pages,
            notifications,
        } => {
            init(&path, &shipped_config())?;
            apply(&path, &poll, &history, &pages, &notifications)
        }
    }
}

fn init(path: &Path, shipped: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let contents = fs::read(shipped).map_err(|error| {
        Error::Runtime(format!(
            "cannot read shipped config {}: {error}",
            shipped.display()
        ))
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::Runtime(format!("user config has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("{}.init", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&contents)?;
        file.flush()?;
        fs::hard_link(&temp, path)
    })();
    let _ = fs::remove_file(&temp);
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.exists() => Ok(()),
        Err(error) => Err(Error::Runtime(format!(
            "cannot initialize user config {}: {error}",
            path.display()
        ))),
    }
}

fn show(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let settings: UiSections = toml::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    let pages = flags(&PAGES, |name| {
        settings.pages.order.iter().any(|page| page == name)
    });
    let notifications = flags(&NOTIFICATIONS, |name| {
        notification(&settings.notifications, name)
    });
    Ok(format!(
        "{}\t{}\t{pages}\t{notifications}\n",
        settings.display.poll_interval.as_secs_f64(),
        settings.display.history_interval.as_secs_f64(),
    ))
}

fn flags<const N: usize>(names: &[&str; N], enabled: impl Fn(&str) -> bool) -> String {
    names
        .iter()
        .map(|name| if enabled(name) { '1' } else { '0' })
        .collect()
}

fn notification(config: &NotificationConfig, name: &str) -> bool {
    match name {
        "disk_usage" => config.disk_usage,
        "disk_smart" => config.disk_smart,
        "cpu_temp" => config.cpu_temp,
        "hd_temp" => config.hd_temp,
        "gpu_nvidia_temp" => config.gpu_nvidia_temp,
        "gpu_amd_temp" => config.gpu_amd_temp,
        "battery_sys" => config.battery_sys,
        "battery_mouse" => config.battery_mouse,
        "battery_kbd" => config.battery_kbd,
        "load_avg" => config.load_avg,
        "server_check" => config.server_check,
        _ => false,
    }
}

fn parse_interval(text: &str) -> Result<f64> {
    let seconds = text
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.1)
        .ok_or_else(|| Error::Runtime("interval must be at least 0.1 seconds".to_owned()))?;
    std::time::Duration::try_from_secs_f64(seconds)
        .map_err(|_| Error::Runtime("interval is too large".to_owned()))?;
    Ok(seconds)
}

fn parse_flags<const N: usize>(text: &str) -> Result<[bool; N]> {
    let mut values = [false; N];
    if text.len() != N {
        return Err(Error::Runtime("invalid widget setting flags".to_owned()));
    }
    for (index, byte) in text.bytes().enumerate() {
        values[index] = match byte {
            b'0' => false,
            b'1' => true,
            _ => return Err(Error::Runtime("invalid widget setting flags".to_owned())),
        };
    }
    Ok(values)
}

fn apply(path: &Path, poll: &str, history: &str, pages: &str, notifications: &str) -> Result<()> {
    let poll = parse_interval(poll)?;
    let history = parse_interval(history)?;
    let pages = parse_flags::<5>(pages)?;
    let notifications = parse_flags::<11>(notifications)?;
    let lock_path = path.with_extension("lock");
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive)
        .map_err(|(_file, errno)| io::Error::from_raw_os_error(errno as i32))?;
    let text = fs::read_to_string(path)?;
    let current: UiSections = toml::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    let current_pages = flags(&PAGES, |name| {
        current.pages.order.iter().any(|page| page == name)
    });
    let current_notifications = flags(&NOTIFICATIONS, |name| {
        notification(&current.notifications, name)
    });
    let desired_pages: String = pages
        .iter()
        .map(|enabled| if *enabled { '1' } else { '0' })
        .collect();
    let desired_notifications: String = notifications
        .iter()
        .map(|enabled| if *enabled { '1' } else { '0' })
        .collect();
    if current.display.poll_interval.as_secs_f64() == poll
        && current.display.history_interval.as_secs_f64() == history
        && current_pages == desired_pages
        && current_notifications == desired_notifications
    {
        return Ok(());
    }
    let mut doc = DocumentMut::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    ensure_table(&mut doc, "display")?;
    ensure_table(&mut doc, "pages")?;
    ensure_table(&mut doc, "notifications")?;
    replace_value(&mut doc["display"]["poll_interval"], Value::from(poll));
    replace_value(
        &mut doc["display"]["history_interval"],
        Value::from(history),
    );
    let mut ordered = current
        .pages
        .order
        .into_iter()
        .filter(|name| {
            PAGES
                .iter()
                .position(|page| page == name)
                .is_none_or(|index| pages[index])
        })
        .collect::<Vec<_>>();
    for (index, name) in PAGES.iter().enumerate() {
        if pages[index] && !ordered.iter().any(|existing| existing == name) {
            ordered.push((*name).to_owned());
        }
    }
    let mut page_array = Array::new();
    for name in ordered {
        page_array.push(name);
    }
    replace_value(&mut doc["pages"]["order"], Value::from(page_array));
    for (index, name) in NOTIFICATIONS.iter().enumerate() {
        replace_value(
            &mut doc["notifications"][*name],
            Value::from(notifications[index]),
        );
    }
    let updated = doc.to_string();
    let _: UiSections = toml::from_str(&updated)
        .map_err(|error| Error::Runtime(format!("invalid updated config: {error}")))?;
    write_config(path, updated.as_bytes()).map_err(Into::into)
}

fn replace_value(item: &mut Item, mut replacement: Value) {
    if let Some(previous) = item.as_value() {
        *replacement.decor_mut() = previous.decor().clone();
    }
    *item = Item::Value(replacement);
}

fn write_config(path: &Path, contents: &[u8]) -> io::Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot save through a symlinked config file",
        ));
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let permissions = fs::metadata(path)?.permissions();
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.set_permissions(permissions)?;
        file.write_all(contents)?;
        file.flush()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn ensure_table(doc: &mut DocumentMut, name: &str) -> Result<()> {
    if doc.get(name).is_none() {
        doc[name] = Item::Table(Table::new());
    }
    if !doc[name].is_table() {
        return Err(Error::Runtime(format!(
            "config section [{name}] must be a table"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
