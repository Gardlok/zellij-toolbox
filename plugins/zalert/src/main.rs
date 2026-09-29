use std::collections::BTreeMap;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Hidden,
    Notice,
    List,
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
    view: View,
    notice: String,
}

impl State {
    fn refresh_session_name(&mut self) {
        let env = get_session_environment_variables();
        self.session_name = env
            .get("ZELLIJ_SESSION_NAME")
            .cloned()
            .unwrap_or_else(|| "zellij".to_owned());
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

    fn run_pending(&mut self) {
        let Some(action) = self.pending_action.take() else {
            return;
        };

        match action.as_str() {
            "watch" => self.toggle_watch(),
            "open" => self.open_list(),
            _ => {}
        }
    }

    fn toggle_watch(&mut self) {
        let Ok((_tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            self.show_notice("zalert: focused pane is not a terminal pane".to_owned());
            return;
        };

        if self.watches.remove(&pane_id).is_some() {
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

        self.watches.insert(
            pane_id,
            Watch {
                pane_id,
                title: title.clone(),
                command: command.clone(),
                phase,
            },
        );

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
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Timer,
            EventType::CommandChanged,
            EventType::PaneClosed,
            EventType::PermissionRequestResult,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
            PermissionType::ReadSessionEnvironmentVariables,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;
                self.refresh_session_name();
                self.run_pending();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_action = None;
                false
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

                        if let Some(active_watch) = self.watches.get_mut(&pane_id) {
                            active_watch.command = command;
                            active_watch.phase = WatchPhase::WaitingForFinish;
                        }

                        self.view == View::List
                    }
                    WatchPhase::WaitingForFinish => {
                        if command == watch.command {
                            return false;
                        }

                        self.notify_command_finished(&watch, &command);
                        self.watches.remove(&pane_id);
                        self.view == View::List
                    }
                }
            }
            Event::PaneClosed(PaneId::Terminal(pane_id)) => {
                let removed = self.watches.remove(&pane_id).is_some();
                removed && self.view == View::List
            }
            Event::Timer(_) if self.view == View::Notice => {
                self.view = View::Hidden;
                hide_self();
                false
            }
            Event::Key(key) if self.view != View::Hidden => {
                if key.has_no_modifiers()
                    && matches!(key.bare_key, BareKey::Esc | BareKey::Char('q'))
                {
                    self.view = View::Hidden;
                    hide_self();
                }
                true
            }
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if matches!(pipe_message.name.as_str(), "watch" | "open") {
            if self.permissions_granted {
                match pipe_message.name.as_str() {
                    "watch" => self.toggle_watch(),
                    "open" => self.open_list(),
                    _ => {}
                }
            } else {
                self.pending_action = Some(pipe_message.name);
            }
            return true;
        }

        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
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
                println!("Alt+W arms/toggles the focused pane. Esc/q: close");
            }
            View::Hidden => {}
        }
    }
}
