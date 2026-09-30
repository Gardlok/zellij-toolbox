use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone)]
struct Mark {
    id: u128,
    revision: u128,
    pane_id: u32,
    tab_index: usize,
    title: String,
    name: Option<String>,
    top_offset: usize,
    cursor_row: usize,
    anchor: String,
    restored: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Hidden,
    Notice,
    List,
    Rename,
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
    view: View,
    marks: Vec<Mark>,
    selected: usize,
    notice: String,
    rename_buffer: String,
    session_name: String,
    companion_path: String,
    state_loaded: bool,
    load_request: u64,
    persistence_error: Option<String>,
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
            .map(|home| format!("{home}/.local/bin/zellij-toolbox-zmark"))
            .unwrap_or_else(|| "zellij-toolbox-zmark".to_owned());
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

    fn next_mark_id(&self) -> u128 {
        let mut id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(1);

        while self.marks.iter().any(|mark| mark.id == id) {
            id = id.saturating_add(1);
        }

        id
    }

    fn next_revision(current: u128) -> u128 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(1);
        now.max(current.saturating_add(1))
    }

    fn request_load(&mut self) {
        self.load_request = self.load_request.saturating_add(1);

        let mut context = BTreeMap::new();
        context.insert("zmark-op".to_owned(), "load".to_owned());
        context.insert("zmark-request".to_owned(), self.load_request.to_string());

        run_command(
            &[&self.companion_path, "list-machine", &self.session_name],
            context,
        );
    }

    fn apply_load_result(
        &mut self,
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) -> bool {
        if context.get("zmark-op").map(String::as_str) != Some("load") {
            return false;
        }

        let request = context
            .get("zmark-request")
            .and_then(|value| value.parse::<u64>().ok());
        if request != Some(self.load_request) {
            return false;
        }

        self.state_loaded = true;

        if exit_code != Some(0) {
            let error = String::from_utf8_lossy(&stderr).trim().to_owned();
            self.persistence_error = Some(if error.is_empty() {
                "durable state helper unavailable".to_owned()
            } else {
                error
            });
            self.run_pending();
            return self.view != View::Hidden;
        }

        let output = String::from_utf8_lossy(&stdout);
        let mut marks = Vec::new();

        for line in output.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 9 {
                continue;
            }

            let Ok(id) = fields[0].parse::<u128>() else {
                continue;
            };
            let Ok(revision) = fields[1].parse::<u128>() else {
                continue;
            };
            let Ok(pane_id) = fields[2].parse::<u32>() else {
                continue;
            };
            let Ok(tab_index) = fields[3].parse::<usize>() else {
                continue;
            };
            let Ok(top_offset) = fields[4].parse::<usize>() else {
                continue;
            };
            let Ok(cursor_row) = fields[5].parse::<usize>() else {
                continue;
            };
            let Some(title) = Self::decode_hex(fields[6]) else {
                continue;
            };
            let Some(name) = Self::decode_hex(fields[7]) else {
                continue;
            };
            let Some(anchor) = Self::decode_hex(fields[8]) else {
                continue;
            };

            marks.push(Mark {
                id,
                revision,
                pane_id,
                tab_index,
                title,
                name: if name.is_empty() { None } else { Some(name) },
                top_offset,
                cursor_row,
                anchor,
                restored: true,
            });
        }

        self.marks = marks;
        self.selected = self.selected.min(self.marks.len().saturating_sub(1));
        self.persistence_error = None;
        self.run_pending();
        self.view != View::Hidden
    }

    fn persistence_context() -> BTreeMap<String, String> {
        BTreeMap::from([("zmark-op".to_owned(), "persist".to_owned())])
    }

    fn persist_put(&self, mark: &Mark) {
        let id = mark.id.to_string();
        let revision = mark.revision.to_string();
        let pane_id = mark.pane_id.to_string();
        let tab_index = mark.tab_index.to_string();
        let top_offset = mark.top_offset.to_string();
        let cursor_row = mark.cursor_row.to_string();
        let name = mark.name.as_deref().unwrap_or("");

        run_command(
            &[
                &self.companion_path,
                "put",
                &self.session_name,
                &id,
                &revision,
                &pane_id,
                &tab_index,
                &top_offset,
                &cursor_row,
                &mark.title,
                name,
                &mark.anchor,
            ],
            Self::persistence_context(),
        );
    }

    fn persist_delete(&self, id: u128, revision: u128) {
        let id = id.to_string();
        let revision = revision.to_string();
        run_command(
            &[
                &self.companion_path,
                "delete",
                &self.session_name,
                &id,
                &revision,
            ],
            Self::persistence_context(),
        );
    }

    fn apply_persist_result(
        &mut self,
        exit_code: Option<i32>,
        stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) -> bool {
        if context.get("zmark-op").map(String::as_str) != Some("persist") {
            return false;
        }

        if exit_code == Some(0) {
            self.persistence_error = None;
        } else {
            let error = String::from_utf8_lossy(&stderr).trim().to_owned();
            self.persistence_error = Some(if error.is_empty() {
                "could not save durable mark state".to_owned()
            } else {
                error
            });
        }

        self.view == View::List
    }

    fn run_pending(&mut self) {
        if !self.state_loaded {
            return;
        }

        let Some(action) = self.pending_action.take() else {
            return;
        };

        match action.as_str() {
            "mark" => self.add_mark(),
            "open" => self.open_list(),
            _ => {}
        }
    }

    fn show_notice(&mut self, notice: String) {
        self.notice = notice;
        self.view = View::Notice;
        show_self(true);
        set_timeout(1.5);
    }

    fn add_mark(&mut self) {
        let Ok((tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            self.show_notice("zmark: focused pane is not a terminal pane".to_owned());
            return;
        };

        let pane = get_pane_info(PaneId::Terminal(pane_id));
        let Ok(contents) = get_pane_scrollback(PaneId::Terminal(pane_id), true) else {
            self.show_notice("zmark: could not read pane scrollback".to_owned());
            return;
        };

        let cursor_row = contents
            .cursor
            .map(|(_, y)| y)
            .unwrap_or(0)
            .min(contents.viewport.len().saturating_sub(1));

        let anchor = contents
            .viewport
            .get(cursor_row)
            .cloned()
            .or_else(|| {
                contents
                    .viewport
                    .iter()
                    .find(|line| !line.trim().is_empty())
                    .cloned()
            })
            .unwrap_or_default();

        let title = pane
            .as_ref()
            .map(|pane| pane.title.clone())
            .unwrap_or_else(|| format!("pane {}", pane_id));

        let id = self.next_mark_id();
        let mark = Mark {
            id,
            revision: id,
            pane_id,
            tab_index,
            title,
            name: None,
            top_offset: contents.lines_above_viewport.len(),
            cursor_row,
            anchor,
            restored: false,
        };

        self.persist_put(&mark);
        self.marks.push(mark);
        self.selected = self.marks.len().saturating_sub(1);
        self.show_notice(format!(
            "Marked pane {} ({} mark(s))",
            pane_id,
            self.marks.len()
        ));
        set_timeout(1.0);
    }

    fn open_list(&mut self) {
        self.selected = self.selected.min(self.marks.len().saturating_sub(1));
        self.view = View::List;
        show_self(true);
    }

    fn begin_rename(&mut self) {
        let Some(mark) = self.marks.get(self.selected) else {
            return;
        };

        self.rename_buffer = mark.name.clone().unwrap_or_default();
        self.view = View::Rename;
    }

    fn finish_rename(&mut self) {
        let name = self.rename_buffer.trim().to_owned();
        let persisted = if let Some(mark) = self.marks.get_mut(self.selected) {
            mark.name = if name.is_empty() { None } else { Some(name) };
            mark.revision = Self::next_revision(mark.revision);
            Some(mark.clone())
        } else {
            None
        };

        if let Some(mark) = persisted {
            self.persist_put(&mark);
        }

        self.rename_buffer.clear();
        self.view = View::List;
    }

    fn jump_to_selected(&mut self) {
        let Some(mark) = self.marks.get(self.selected).cloned() else {
            return;
        };

        let pane_id = PaneId::Terminal(mark.pane_id);
        let Some(info) = get_pane_info(pane_id) else {
            self.show_notice(format!(
                "Stale mark: pane {} no longer exists",
                mark.pane_id
            ));
            return;
        };

        let Ok(contents) = get_pane_scrollback(pane_id, true) else {
            self.show_notice(format!(
                "Stale mark: pane {} scrollback is unavailable",
                mark.pane_id
            ));
            return;
        };

        let mut lines = contents.lines_above_viewport;
        lines.extend(contents.viewport);
        lines.extend(contents.lines_below_viewport);

        let mut target_top = mark.top_offset;
        let mut anchor_resolved = false;

        if !mark.anchor.is_empty() {
            let original_anchor_index = mark.top_offset + mark.cursor_row;
            if let Some((match_index, _)) = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| **line == mark.anchor)
                .min_by_key(|(index, _)| index.abs_diff(original_anchor_index))
            {
                target_top = match_index.saturating_sub(mark.cursor_row);
                anchor_resolved = true;
            }
        }

        if mark.restored && !anchor_resolved {
            self.show_notice(format!(
                "Stale mark: saved anchor for pane {} could not be re-resolved",
                mark.pane_id
            ));
            return;
        }

        self.view = View::Hidden;
        hide_self();
        focus_terminal_pane(mark.pane_id, false, false);
        scroll_to_top_in_pane_id(pane_id);

        let page_size = info.pane_content_rows.saturating_sub(1).max(1);
        let page_count = target_top / page_size;
        let remainder = target_top % page_size;

        for _ in 0..page_count {
            page_scroll_down_in_pane_id(pane_id);
        }
        for _ in 0..remainder {
            scroll_down_in_pane_id(pane_id);
        }
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
                BareKey::Up | BareKey::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.marks.is_empty() {
                        self.selected = (self.selected + 1).min(self.marks.len() - 1);
                    }
                }
                BareKey::Enter => self.jump_to_selected(),
                BareKey::Char('n') => self.begin_rename(),
                BareKey::Char('d') => {
                    if !self.marks.is_empty() {
                        let removed = self.marks.remove(self.selected);
                        let revision = Self::next_revision(removed.revision);
                        self.persist_delete(removed.id, revision);
                        self.selected = self.selected.min(self.marks.len().saturating_sub(1));
                    }
                }
                _ => {}
            },
            View::Rename => match key.bare_key {
                BareKey::Esc => {
                    self.rename_buffer.clear();
                    self.view = View::List;
                }
                BareKey::Enter => self.finish_rename(),
                BareKey::Backspace => {
                    self.rename_buffer.pop();
                }
                BareKey::Char(c) => self.rename_buffer.push(c),
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
            EventType::PermissionRequestResult,
            EventType::RunCommandResult,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadPaneContents,
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
                self.request_load();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_action = None;
                false
            }
            Event::RunCommandResult(exit_code, stdout, stderr, context) => {
                if context.get("zmark-op").map(String::as_str) == Some("load") {
                    self.apply_load_result(exit_code, stdout, stderr, context)
                } else {
                    self.apply_persist_result(exit_code, stderr, context)
                }
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
        if matches!(pipe_message.name.as_str(), "mark" | "open") {
            if self.permissions_granted && self.state_loaded {
                match pipe_message.name.as_str() {
                    "mark" => self.add_mark(),
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

    fn render(&mut self, rows: usize, cols: usize) {
        match self.view {
            View::Notice => {
                println!("zmark");
                println!();
                println!("{}", self.notice);
            }
            View::List => {
                println!("zmark — durable bookmarks");
                if let Some(error) = &self.persistence_error {
                    println!("Persistence unavailable: {}", error);
                }
                println!();

                if self.marks.is_empty() {
                    println!("No marks yet.");
                } else {
                    let reserved = if self.persistence_error.is_some() {
                        6
                    } else {
                        5
                    };
                    let available = rows.saturating_sub(reserved).max(1);
                    let start = self.selected.saturating_sub(available.saturating_sub(1));
                    let end = (start + available).min(self.marks.len());

                    for (index, mark) in self.marks[start..end].iter().enumerate() {
                        let absolute_index = start + index;
                        let marker = if absolute_index == self.selected {
                            ">"
                        } else {
                            " "
                        };
                        let anchor = mark.anchor.trim();
                        let name = mark.name.as_deref().unwrap_or("(unnamed)");
                        let prefix = format!(
                            "{} {}. {} — T{} P{} {} — ",
                            marker,
                            absolute_index + 1,
                            name,
                            mark.tab_index + 1,
                            mark.pane_id,
                            mark.title
                        );
                        let width = cols.saturating_sub(prefix.chars().count());
                        let anchor: String = anchor.chars().take(width).collect();
                        println!("{}{}", prefix, anchor);
                    }
                }

                println!();
                println!("Up/Down: select   Enter: jump   n: name/rename   d: delete   Esc: close");
            }
            View::Rename => {
                println!("zmark — name bookmark");
                println!();
                println!("Name: {}", self.rename_buffer);
                println!();
                println!("Enter: save   Esc: cancel");
            }
            View::Hidden => {}
        }
    }
}
