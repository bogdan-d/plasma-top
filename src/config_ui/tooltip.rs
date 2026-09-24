//! Editable main-tooltip layout for the widget configuration page.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;
use std::str::FromStr;

use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::{replace_value, write_config};
use crate::domain::registry::list_items;
use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TooltipSection {
    key: String,
    title: String,
    enabled: bool,
    items: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct TooltipEdit {
    sections: Vec<TooltipSection>,
}

#[derive(Serialize, Deserialize)]
struct TooltipSnapshot {
    sections: Vec<TooltipSection>,
    available: Vec<String>,
}

pub(super) fn show(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let doc = DocumentMut::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    serde_json::to_string(&TooltipSnapshot {
        sections: sections(&doc)?,
        available: available_items(),
    })
    .map_err(|error| Error::Runtime(format!("cannot encode tooltip settings: {error}")))
}

fn available_items() -> Vec<String> {
    let mut items = list_items()
        .into_iter()
        .filter(|(_, placement)| *placement != "panel only")
        .map(|(token, _)| token)
        .collect::<Vec<_>>();
    items.extend(["separator_small".to_owned(), "separator_big".to_owned()]);
    items.sort();
    items
}

fn strings(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn tooltip_table(doc: &DocumentMut) -> Result<&Table> {
    doc.get("tooltip")
        .and_then(Item::as_table)
        .ok_or_else(|| Error::Runtime("config section [tooltip] must be a table".to_owned()))
}

fn sections(doc: &DocumentMut) -> Result<Vec<TooltipSection>> {
    let table = tooltip_table(doc)?;
    let order = strings(table.get("order"));
    let mut keys = Vec::new();
    for key in &order {
        if table.get(key).is_some_and(Item::is_table) && !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    for (key, item) in table.iter() {
        if item.is_table() && !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_owned());
        }
    }
    Ok(keys
        .into_iter()
        .filter_map(|key| {
            let section = table.get(&key)?.as_table()?;
            Some(TooltipSection {
                enabled: order.contains(&key),
                title: section
                    .get("title")
                    .and_then(Item::as_str)
                    .unwrap_or(&key)
                    .to_owned(),
                items: strings(section.get("items")),
                key,
            })
        })
        .collect())
}

pub(super) fn apply(path: &Path, payload: &str) -> Result<()> {
    if payload.len() > 65_536 {
        return Err(Error::Runtime("tooltip settings are too large".to_owned()));
    }
    let requested: TooltipEdit = serde_json::from_str(payload)
        .map_err(|error| Error::Runtime(format!("invalid tooltip settings: {error}")))?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive)
        .map_err(|(_file, errno)| io::Error::from_raw_os_error(errno as i32))?;
    let text = fs::read_to_string(path)?;
    let mut doc = DocumentMut::from_str(&text)
        .map_err(|error| Error::Runtime(format!("invalid user config: {error}")))?;
    let current = sections(&doc)?;
    let current_keys = current
        .iter()
        .map(|section| section.key.as_str())
        .collect::<HashSet<_>>();
    let requested_keys = requested
        .sections
        .iter()
        .map(|section| section.key.as_str())
        .collect::<HashSet<_>>();
    if requested_keys != current_keys || requested_keys.len() != requested.sections.len() {
        return Err(Error::Runtime(
            "tooltip sections changed; reload the settings page".to_owned(),
        ));
    }
    let available = available_items().into_iter().collect::<HashSet<_>>();
    for section in &requested.sections {
        let previous = current.iter().find(|item| item.key == section.key);
        for item in &section.items {
            if !available.contains(item)
                && !previous.is_some_and(|previous| previous.items.contains(item))
            {
                return Err(Error::Runtime(format!(
                    "item {item:?} is not available on the tooltip"
                )));
            }
        }
    }
    let original_order = strings(tooltip_table(&doc)?.get("order"));
    let mut desired_order = requested
        .sections
        .iter()
        .filter(|section| section.enabled)
        .map(|section| section.key.clone())
        .collect::<Vec<_>>();
    desired_order.extend(
        original_order
            .iter()
            .filter(|key| !current_keys.contains(key.as_str()))
            .cloned(),
    );
    let mut changed = desired_order != original_order;
    if changed {
        replace_value(
            &mut doc["tooltip"]["order"],
            Value::from(array(&desired_order)),
        );
    }
    for section in &requested.sections {
        let previous = current.iter().find(|item| item.key == section.key);
        if previous.is_some_and(|previous| previous.items != section.items) {
            replace_value(
                &mut doc["tooltip"][section.key.as_str()]["items"],
                Value::from(array(&section.items)),
            );
            changed = true;
        }
    }
    if changed {
        let updated = doc.to_string();
        let _: toml::Value = toml::from_str(&updated)
            .map_err(|error| Error::Runtime(format!("invalid updated config: {error}")))?;
        write_config(path, updated.as_bytes())?;
    }
    Ok(())
}

fn array(items: &[String]) -> Array {
    let mut array = Array::new();
    for item in items {
        array.push(item.as_str());
    }
    array
}

#[cfg(test)]
mod tests;
