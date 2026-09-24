//! Editable Graphs page layout for the widget configuration page.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;
use std::str::FromStr;

use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Value};

use super::{UiSections, ensure_table, replace_value, write_config};
use crate::config::GRAPH_CHARTS;
use crate::error::{Error, Result};

#[derive(Serialize, Deserialize)]
struct GraphEdit {
    order: Vec<String>,
    history_length: i32,
}

#[derive(Serialize, Deserialize)]
struct GraphSnapshot {
    order: Vec<String>,
    history_length: i32,
    available: Vec<String>,
}

pub(super) fn show(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let settings: UiSections = toml::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    serde_json::to_string(&GraphSnapshot {
        order: settings.pages.graph_order,
        history_length: settings.pages.graph_history_length,
        available: GRAPH_CHARTS
            .iter()
            .map(|chart| (*chart).to_owned())
            .collect(),
    })
    .map_err(|error| Error::Runtime(format!("cannot encode Graphs settings: {error}")))
}

pub(super) fn apply(path: &Path, payload: &str) -> Result<()> {
    if payload.len() > 4096 {
        return Err(Error::Runtime("Graphs settings are too large".to_owned()));
    }
    let requested: GraphEdit = serde_json::from_str(payload)
        .map_err(|error| Error::Runtime(format!("invalid Graphs settings: {error}")))?;
    let unique = requested.order.iter().collect::<HashSet<_>>();
    if unique.len() != requested.order.len()
        || requested
            .order
            .iter()
            .any(|chart| !GRAPH_CHARTS.contains(&chart.as_str()))
    {
        return Err(Error::Runtime(
            "Graphs charts must be unique and available".to_owned(),
        ));
    }
    if !(1..=10_000).contains(&requested.history_length) {
        return Err(Error::Runtime(
            "Graphs history length must be between 1 and 10000 samples".to_owned(),
        ));
    }

    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive)
        .map_err(|(_file, errno)| io::Error::from_raw_os_error(errno as i32))?;
    let text = fs::read_to_string(path)?;
    let current: UiSections = toml::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    if current.pages.graph_order == requested.order
        && current.pages.graph_history_length == requested.history_length
    {
        return Ok(());
    }

    let mut doc = DocumentMut::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    ensure_table(&mut doc, "pages")?;
    let mut order = Array::new();
    for chart in &requested.order {
        order.push(chart.as_str());
    }
    replace_value(&mut doc["pages"]["graph_order"], Value::from(order));
    replace_value(
        &mut doc["pages"]["graph_history_length"],
        Value::from(i64::from(requested.history_length)),
    );
    let updated = doc.to_string();
    let _: UiSections = toml::from_str(&updated)
        .map_err(|error| Error::Runtime(format!("invalid updated config: {error}")))?;
    write_config(path, updated.as_bytes()).map_err(Into::into)
}

#[cfg(test)]
mod tests;
