use std::collections::HashSet;

use promkit_core::crossterm::{
    event::{Event, KeyEvent, MouseEvent},
    style::ContentStyle,
};
use promkit_widgets::{listbox, text_editor};
use serde::{Deserialize, Serialize};
use termcfg::crossterm_config::{content_style_serde, event_set_serde};

pub static DEFAULT_CONFIG: &str = include_str!("../default.toml");

#[derive(Clone, Serialize, Deserialize)]
pub struct EditorKeybinds {
    #[serde(with = "event_set_serde")]
    pub backward: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub forward: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub move_to_head: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub move_to_tail: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub erase: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub erase_all: HashSet<Event>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StreamingKeybinds {
    #[serde(with = "event_set_serde")]
    pub exit: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub goto_archived: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub retry: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub toggle_pause: HashSet<Event>,
    pub editor: EditorKeybinds,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchivedKeybinds {
    #[serde(with = "event_set_serde")]
    pub exit: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub retry: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub up: HashSet<Event>,
    #[serde(with = "event_set_serde")]
    pub down: HashSet<Event>,
    pub editor: EditorKeybinds,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    pub editor: text_editor::Config,
    pub keybinds: StreamingKeybinds,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchivedConfig {
    pub editor: text_editor::Config,
    pub listbox: listbox::Config,
    pub keybinds: ArchivedKeybinds,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub streaming: StreamingConfig,
    pub archived: ArchivedConfig,
    #[serde(with = "content_style_serde")]
    pub highlight_style: ContentStyle,
}

impl Config {
    pub fn load_from(content: &str) -> anyhow::Result<Self> {
        toml::from_str(content).map_err(Into::into)
    }
}

pub fn matches_keybind(event: &Event, keybinds: &HashSet<Event>) -> bool {
    let normalized = match event {
        Event::Key(key) => Event::Key(KeyEvent::new(key.code, key.modifiers)),
        Event::Mouse(mouse) => Event::Mouse(MouseEvent {
            kind: mouse.kind,
            column: 0,
            row: 0,
            modifiers: mouse.modifiers,
        }),
        other => other.clone(),
    };

    keybinds.contains(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_toml() {
        Config::load_from(DEFAULT_CONFIG).expect("default.toml must be valid");
    }
}
