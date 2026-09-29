use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone, Copy, PartialEq, Eq)]
enum WatchPhase {
    WaitingForCommand,
    WaitingForFinish,
}

#[derive(Clone)]
struct Watch {
    pane_id: u32,
    title: String,
    command: Vec<String>,
    phase: WatchPhase,
}

#[derive(Clone)]
struct GlobalWatch {
    session: String,
    pane_id: u32,
    phase: String,
    title: String,
    command: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Hidden,
    Notice,
    List,
    GlobalLoading,
    GlobalList,
}

impl Default for View {
    fn default() -> Self {
        Self::Hidden
    }
}

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_action: Option<String>,
    watches: BTreeMap<u32, Watch>,
    session_name: String,
    companion_path: String,
    companion_generation: u128,
    companion_revision: u64,
    view: View,
    notice: String,
    global_watches: Vec<GlobalWatch>,
    global_selected: usize,
    global_status: String,
    global_request: u64,
}

impl State {
    fn refresh_environment(&mut self) {
        let env = get_session_environment_variables();
        self.session_name = env
            .get("ZELLIJ_SESSION_NAME")
            .cloned()
            .unwrap_or_else(|| "zellij".to_owned());

        self.companion_path = env
            .get("HOME")
            .map(|home| format!("{home}/.local/bin/zellij-toolbox-alert"))
            .unwrap_or_else(|| "zellij-toolbox-alert".to_owned());

        self.companion_generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        self.companion_revision = 0;
    }

    fn next_revision(&mut self) -> u64 {
        self.companion_revision = self.companion_revision.saturating_add(1);
        self.companion_revision
    }

    fn command_is_shell(command: &[String]) -> bool {
        let Some(program) = command.first() else {
            return true;
        };

        let program = program.rsplit('/').next().unwrap_or(program.as_str());
        matches!(
            program,
            "bash" | "zsh" | "fish" | "sh" | "dash" | "ksh" | "mksh"
        )
    }

    fn command_label(command: &[String]) -> String {
        if command.is_empty() {
            "(unknown command)".to_owned()
        } else {
            command.join(" ")
        }
    }

    fn decode_hex(value: &str) -> Option<String> {
        if value.len() % 2 != 0 {
            return None;
        }

        let mut bytes = Vec::with_capacity(value.len() / 2);
        let mut index = 0;
        while index < value.len() {
            let byte = u8::from_str_radix(&value[index..index + 2], 16).ok()?;
            bytes.push(byte);
            index += 2;
        }

        String::from_utf8(bytes).ok()
    }

    fn companion_touch(&self) {
        let generation = self.companion_generation.to_string();
        run_command(
            &[
                &self.companion_path,
                "touch",
                &self.session_name,
                &generation,
            ],
            BTreeMap::new(),
        );
    }

    fn companion_publish(&mut self, watch: &Watch) {
        let pane_id = watch.pane_id.to_string();
        let generation = self.companion_generation.to_string();
        let revision = self.next_revision().to_string();
        let phase = match watch.phase {
            WatchPhase::WaitingForCommand => "armed",
            WatchPhase::WaitingForFinish => "running",
        };
        let command = match watch.phase {
            WatchPhase::WaitingForCommand => String::new(),
            WatchPhase::WaitingForFinish => Self::command_label(&watch.command),
        };

        run_command(
            &[
                &self.companion_path,
                "upsert",
                &self.session_name,
                &pane_id,
                &generation,
                &revision,
                phase,
                &watch.title,
                &command,
            ],
            BTreeMap::new(),
        );
    }

    fn companion_clear(&mut self, pane_id: u32) {
        let pane_id = pane_id.to_string();
        let generation = self.companion_generation.to_string();
        let revision = self.next_revision().to_string();

        run_command(
            &[
                &self.companion_path,
                "clear",
                &self.session_name,
                &pane_id,
                &generation,
                &revision,
            ],
            BTreeMap::new(),
        );
    }

    fn request_global_list(&mut self) {
        self.global_request = self.global_request.saturating_add(1);
        self.global_status = "Loading cross-session watches...".to_owned();

        let mut context = BTreeMap::new();
        context.insert("zalert-op".to_owned(), "global-list".to_owned());
        context.insert(
            "zalert-request".to_owned(),
            self.global_request.to_string(),
        );

        let generation = self.companion_generation.to_string();
        run_command(
            &[
                &self.companion_path,
                "list-machine",
                &self.session_name,
                &generation,
            ],
            context,
        );
    }

    fn apply_global_list_result(
        &mut self,
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) -> bool {
        if context.get("zalert-op").map(String::as_str) != Some("global-list") {
            return false;
        }

        let request = context
            .get("zalert-request")
            .and_then(|value| value.parse::<u64>().ok());
        if request != Some(self.global_request) {
            return false;
        }

        if exit_code != Some(0) {
            let error = String::from_utf8_lossy(&stderr).trim().to_owned();
            self.global_watches.clear();
            self.global_selected = 0;
            self.global_status = if error.is_empty() {
                "Could not read cross-session watches.".to_owned()
            } else {
                format!("Companion error: {error}")
            };
            self.view = View::GlobalList;
            return true;
        }

        let output = String::from_utf8_lossy(&stdout);
        let mut watches = Vec::new();

        for line in output.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 5 {
                continue;
            }

            let Some(session) = Self::decode_hex(fields[0]) else {
                continue;
            };
            let Ok(pane_id) = fields[1].parse::<u32>() else {
                continue;
            };
            let phase = fields[2].to_owned();
            if phase != "armed" && phase != "running" {
                continue;
            }
            let Some(title) = Self::decode_hex(fields[3]) else {
                continue;
            };
            let Some(command) = Self::decode_hex(fields[4]) else {
                continue;
            };

            watches.push(GlobalWatch {
                session,
                pane_id,
                phase,
                title,
                command,
            });
        }

        self.global_watches = watches;
        self.global_selected = self
            .global_selected
            .min(self.global_watches.len().saturating_sub(1));
        self.global_status = if self.global_watches.is_empty() {
            "No active zalert watches across local sessions.".to_owned()
        } else {
            format!("{} active watch(es).", self.global_watches.len())
        };
        self.view = View::GlobalList;
        true
    }

    fn run_pending(&mut self) {
        let Some(action) = self.pending_action.take() else {
            return;
        };

        match action.as_str() {
            "watch" => self.toggle_watch(),
            "open" => self.open_list(),
            "global" => self.open_global_list(),
            _ => {}
        }
    }

    fn toggle_watch(&mut self) {
        let Ok((_tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            self.show_notice("zalert: focused pane is not a terminal pane".to_owned());
            return;
        };

        if self.watches.remove(&pane_id).is_some() {
            self.companion_clear(pane_id);
            self.show_notice(format!("Stopped watching pane {}", pane_id));
            return;
        }

        let pane_id_typed = PaneId::Terminal(pane_id);
        let command = get_pane_running_command(pane_id_typed).unwrap_or_default();
        let title = get_pane_info(pane_id_typed)
            .map(|pane| pane.title)
            .unwrap_or_else(|| format!("pane {}", pane_id));

        let phase = if Self::command_is_shell(&command) {
            WatchPhase::WaitingForCommand
        } else {
            WatchPhase::WaitingForFinish
        };

        let watch = Watch {
            pane_id,
            title: title.clone(),
            command: command.clone(),
            phase,
        };

        self.watches.insert(pane_id, watch.clone());
        self.companion_publish(&watch);

        match phase {
            WatchPhase::WaitingForCommand => {
                self.show_notice(format!("Armed {} for the next command", title));
            }
            WatchPhase::WaitingForFinish => {
                self.show_notice(format!(
                    "Watching {}: {}",
                    title,
                    Self::command_label(&command)
                ));
            }
        }
    }

    fn open_list(&mut self) {
        self.view = View::List;
        show_self(true);
    }

    fn open_global_list(&mut self) {
        self.global_selected = 0;
        self.view = View::GlobalLoading;
        show_self(true);
        self.request_global_list();
    }

    fn jump_to_global_selected(&mut self) {
        let Some(watch) = self.global_watches.get(self.global_selected).cloned() else {
            return;
        };

        self.view = View::Hidden;
        hide_self();

        if watch.session == self.session_name {
            focus_terminal_pane(watch.pane_id, false, false);
        } else {
            switch_session_with_focus(&watch.session, None, Some((watch.pane_id, false)));
        }
    }

    fn show_notice(&mut self, notice: String) {
        self.notice = notice;
        self.view = View::Notice;
        show_self(true);
        set_timeout(1.4);
    }

    fn notify_command_finished(&self, watch: &Watch, new_command: &[String]) {
        let old_command = Self::command_label(&watch.command);
        let new_command = if new_command.is_empty() {
            "(none)".to_owned()
        } else {
            new_command.join(" ")
        };

        let summary = format!("zalert — {}", self.session_name);
        let body = format!(
            "{} (pane {}) finished/changed\n{}  →  {}",
            watch.title, watch.pane_id, old_command, new_command
        );

        run_command(
            &[
                "notify-send",
                "--app-name",
                "zellij-toolbox",
                &summary,
                &body,
            ],
            BTreeMap::new(),
        );
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return true;
        }

        match self.view {
            View::List => match key.bare_key {
                BareKey::Esc | BareKey::Char('q') => {
                    self.view = View::Hidden;
                    hide_self();
                }
                BareKey::Char('g') => self.open_global_list(),
                _ => {}
            },
            View::GlobalLoading => {
                if matches!(key.bare_key, BareKey::Esc | BareKey::Char('q')) {
                    self.view = View::Hidden;
                    hide_self();
                }
            }
            View::GlobalList => match key.bare_key {
                BareKey::Esc | BareKey::Char('q') => {
                    self.view = View::Hidden;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') => {
                    self.global_selected = self.global_selected.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.global_watches.is_empty() {
                        self.global_selected =
                            (self.global_selected + 1).min(self.global_watches.len() - 1);
                    }
                }
                BareKey::Enter => self.jump_to_global_selected(),
                BareKey::Char('r') => {
                    self.view = View::GlobalLoading;
                    self.request_global_list();
                }
                BareKey::Char('l') => self.open_list(),
                _ => {}
            },
            View::Notice => {
                self.view = View::Hidden;
                hide_self();
            }
            View::Hidden => {}
        }

        true
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Timer,
            EventType::CommandChanged,
            EventType::PaneClosed,
            EventType::PermissionRequestResult,
            EventType::RunCommandResult,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
            PermissionType::ReadSessionEnvironmentVariables,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;
                self.refresh_environment();
                self.companion_touch();
                self.run_pending();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_action = None;
                false
            }
            Event::RunCommandResult(exit_code, stdout, stderr, context) => {
                self.apply_global_list_result(exit_code, stdout, stderr, context)
            }
            Event::CommandChanged(PaneId::Terminal(pane_id), command, _is_foreground, _) => {
                let Some(watch) = self.watches.get(&pane_id).cloned() else {
                    return false;
                };

                match watch.phase {
                    WatchPhase::WaitingForCommand => {
                        if Self::command_is_shell(&command) {
                            return false;
                        }

                        let updated_watch =
                            if let Some(active_watch) = self.watches.get_mut(&pane_id) {
                                active_watch.command = command;
                                active_watch.phase = WatchPhase::WaitingForFinish;
                                Some(active_watch.clone())
                            } else {
                                None
                            };

                        if let Some(updated_watch) = updated_watch {
                            self.companion_publish(&updated_watch);
                        }

                        self.view == View::List
                    }
                    WatchPhase::WaitingForFinish => {
                        if command == watch.command {
                            return false;
                        }

                        self.notify_command_finished(&watch, &command);
                        self.companion_clear(pane_id);
                        self.watches.remove(&pane_id);
                        self.view == View::List
                    }
                }
            }
            Event::PaneClosed(PaneId::Terminal(pane_id)) => {
                let removed = self.watches.remove(&pane_id).is_some();
                if removed {
                    self.companion_clear(pane_id);
                }
                removed && self.view == View::List
            }
            Event::Timer(_) if self.view == View::Notice => {
                self.view = View::Hidden;
                hide_self();
                false
            }
            Event::Key(key) if self.view != View::Hidden => self.handle_key(key),
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if matches!(pipe_message.name.as_str(), "watch" | "open" | "global") {
            if self.permissions_granted {
                match pipe_message.name.as_str() {
                    "watch" => self.toggle_watch(),
                    "open" => self.open_list(),
                    "global" => self.open_global_list(),
                    _ => {}
                }
            } else {
                self.pending_action = Some(pipe_message.name);
            }
            return true;
        }

        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match self.view {
            View::Notice => {
                println!("zalert");
                println!();
                println!("{}", self.notice);
            }
            View::List => {
                println!("zalert — {} — active watches", self.session_name);
                println!();

                if self.watches.is_empty() {
                    println!("No panes are being watched.");
                } else {
                    for watch in self.watches.values() {
                        match watch.phase {
                            WatchPhase::WaitingForCommand => {
                                println!(
                                    "P{} {} — armed, waiting for next command",
                                    watch.pane_id, watch.title
                                );
                            }
                            WatchPhase::WaitingForFinish => {
                                println!(
                                    "P{} {} — watching {}",
                                    watch.pane_id,
                                    watch.title,
                                    Self::command_label(&watch.command)
                                );
                            }
                        }
                    }
                }

                println!();
                println!("g: global watches   Alt+W: toggle focused pane   Esc/q: close");
            }
            View::GlobalLoading => {
                println!("zalert — cross-session watches");
                println!();
                println!("{}", self.global_status);
                println!();
                println!("Esc/q: close");
            }
            View::GlobalList => {
                println!("zalert — cross-session watches");
                println!("{}", self.global_status);
                println!();

                let available = rows.saturating_sub(6).max(1);
                let start = self
                    .global_selected
                    .saturating_sub(available.saturating_sub(1));
                let end = (start + available).min(self.global_watches.len());

                for (index, watch) in self.global_watches[start..end].iter().enumerate() {
                    let absolute_index = start + index;
                    let marker = if absolute_index == self.global_selected {
                        ">"
                    } else {
                        " "
                    };
                    let detail = if watch.phase == "armed" {
                        "waiting for next command"
                    } else if watch.command.is_empty() {
                        "running command"
                    } else {
                        &watch.command
                    };
                    let prefix = format!(
                        "{} {} P{} {:<7} {} — ",
                        marker, watch.session, watch.pane_id, watch.phase, watch.title
                    );
                    let width = cols.saturating_sub(prefix.chars().count());
                    let detail: String = detail.chars().take(width).collect();
                    println!("{}{}", prefix, detail);
                }

                println!();
                println!(
                    "Up/Down: select   Enter: jump   r: refresh   l: local   Esc/q: close"
                );
            }
            View::Hidden => {}
        }
    }
}
