use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone)]
struct PaneChoice {
    pane_id: u32,
    tab_index: usize,
    title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Select,
    Input,
    Confirm,
    Sent,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Select
    }
}

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    pane_manifest: PaneManifest,
    choices: Vec<PaneChoice>,
    selected_index: usize,
    selected_panes: BTreeSet<u32>,
    mode: Mode,
    command: String,
    status: String,
}

impl State {
    fn rebuild_choices(&mut self) {
        self.choices.clear();

        let mut tabs: Vec<usize> = self.pane_manifest.panes.keys().copied().collect();
        tabs.sort_unstable();

        for tab_index in tabs {
            if let Some(panes) = self.pane_manifest.panes.get(&tab_index) {
                for pane in panes {
                    if !pane.is_plugin && pane.is_selectable {
                        self.choices.push(PaneChoice {
                            pane_id: pane.id,
                            tab_index,
                            title: pane.title.clone(),
                        });
                    }
                }
            }
        }

        let valid_ids: BTreeSet<u32> = self.choices.iter().map(|pane| pane.pane_id).collect();
        self.selected_panes.retain(|pane_id| valid_ids.contains(pane_id));
        self.selected_index = self
            .selected_index
            .min(self.choices.len().saturating_sub(1));
    }

    fn open(&mut self) {
        self.pending_open = false;
        self.visible = true;
        self.mode = Mode::Select;
        self.command.clear();
        self.status = "Select targets, then press Enter.".to_owned();
        self.selected_panes.clear();
        self.selected_index = 0;
        self.rebuild_choices();
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }

    fn toggle_selected(&mut self) {
        let Some(choice) = self.choices.get(self.selected_index) else {
            return;
        };

        if !self.selected_panes.remove(&choice.pane_id) {
            self.selected_panes.insert(choice.pane_id);
        }
    }

    fn send(&mut self) {
        if self.selected_panes.is_empty() || self.command.trim().is_empty() {
            self.mode = Mode::Input;
            self.status = "Nothing sent: choose panes and enter a command.".to_owned();
            return;
        }

        let payload = format!("{}\n", self.command);
        for pane_id in &self.selected_panes {
            write_chars_to_pane_id(&payload, PaneId::Terminal(*pane_id));
        }

        self.status = format!(
            "Sent to {} pane(s): {}",
            self.selected_panes.len(),
            self.command
        );
        self.mode = Mode::Sent;
        set_timeout(1.5);
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return true;
        }

        match self.mode {
            Mode::Select => match key.bare_key {
                BareKey::Esc | BareKey::Char('q') => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') => {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.choices.is_empty() {
                        self.selected_index =
                            (self.selected_index + 1).min(self.choices.len() - 1);
                    }
                }
                BareKey::Char(' ') => self.toggle_selected(),
                BareKey::Enter => {
                    if self.selected_panes.is_empty() {
                        self.status = "Select at least one pane first.".to_owned();
                    } else {
                        self.mode = Mode::Input;
                        self.status = "Type the command to broadcast.".to_owned();
                    }
                }
                _ => {}
            },
            Mode::Input => match key.bare_key {
                BareKey::Esc => {
                    self.mode = Mode::Select;
                    self.status = "Select targets, then press Enter.".to_owned();
                }
                BareKey::Backspace => {
                    self.command.pop();
                }
                BareKey::Enter => {
                    if self.command.trim().is_empty() {
                        self.status = "Command is empty.".to_owned();
                    } else {
                        self.mode = Mode::Confirm;
                        self.status =
                            "Press Enter again to SEND, or Esc to edit.".to_owned();
                    }
                }
                BareKey::Char(c) => self.command.push(c),
                _ => {}
            },
            Mode::Confirm => match key.bare_key {
                BareKey::Enter => self.send(),
                BareKey::Esc => {
                    self.mode = Mode::Input;
                    self.status = "Edit the command, then press Enter.".to_owned();
                }
                _ => {}
            },
            Mode::Sent => {
                self.visible = false;
                hide_self();
            }
        }

        true
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::PaneUpdate,
            EventType::Timer,
            EventType::PermissionRequestResult,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::WriteToStdin,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;
                if self.pending_open {
                    self.open();
                    return true;
                }
                false
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_open = false;
                false
            }
            Event::PaneUpdate(manifest) => {
                self.pane_manifest = manifest;
                if self.visible && self.mode == Mode::Select {
                    self.rebuild_choices();
                    return true;
                }
                false
            }
            Event::Timer(_) if self.mode == Mode::Sent => {
                self.visible = false;
                hide_self();
                false
            }
            Event::Key(key) if self.visible => self.handle_key(key),
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "open" {
            self.request_open();
            return true;
        }
        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        println!("zbroadcast");
        println!("{}", self.status);
        println!();

        match self.mode {
            Mode::Select => {
                let available = rows.saturating_sub(6).max(1);
                let start = self
                    .selected_index
                    .saturating_sub(available.saturating_sub(1));
                let end = (start + available).min(self.choices.len());

                for (index, choice) in self.choices[start..end].iter().enumerate() {
                    let absolute_index = start + index;
                    let cursor = if absolute_index == self.selected_index {
                        ">"
                    } else {
                        " "
                    };
                    let checked = if self.selected_panes.contains(&choice.pane_id) {
                        "x"
                    } else {
                        " "
                    };
                    println!(
                        "{} [{}] T{} P{} {}",
                        cursor,
                        checked,
                        choice.tab_index + 1,
                        choice.pane_id,
                        choice.title
                    );
                }

                println!();
                println!("Up/Down: move   Space: toggle   Enter: compose   Esc: close");
            }
            Mode::Input => {
                println!("Targets: {} pane(s)", self.selected_panes.len());
                println!();
                let prefix = "Command: ";
                let width = cols.saturating_sub(prefix.chars().count());
                let command: String = self.command.chars().take(width).collect();
                println!("{}{}", prefix, command);
                println!();
                println!("Enter: review   Esc: back");
            }
            Mode::Confirm => {
                println!("Targets: {} pane(s)", self.selected_panes.len());
                println!("Command: {}", self.command);
                println!();
                println!("This will write the command plus Enter to every selected pane.");
                println!();
                println!("Enter: SEND   Esc: edit");
            }
            Mode::Sent => {
                println!("{}", self.status);
            }
        }
    }
}
