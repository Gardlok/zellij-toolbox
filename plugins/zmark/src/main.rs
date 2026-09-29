use std::collections::BTreeMap;
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone)]
struct Mark {
    pane_id: u32,
    tab_index: usize,
    title: String,
    name: Option<String>,
    top_offset: usize,
    cursor_row: usize,
    anchor: String,
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
}

impl State {
    fn run_pending(&mut self) {
        let Some(action) = self.pending_action.take() else {
            return;
        };

        match action.as_str() {
            "mark" => self.add_mark(),
            "open" => self.open_list(),
            _ => {}
        }
    }

    fn add_mark(&mut self) {
        let Ok((tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            self.notice = "zmark: focused pane is not a terminal pane".to_owned();
            self.view = View::Notice;
            show_self(true);
            set_timeout(1.5);
            return;
        };

        let pane = get_pane_info(PaneId::Terminal(pane_id));
        let Ok(contents) = get_pane_scrollback(PaneId::Terminal(pane_id), true) else {
            self.notice = "zmark: could not read pane scrollback".to_owned();
            self.view = View::Notice;
            show_self(true);
            set_timeout(1.5);
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

        self.marks.push(Mark {
            pane_id,
            tab_index,
            title,
            name: None,
            top_offset: contents.lines_above_viewport.len(),
            cursor_row,
            anchor,
        });

        self.selected = self.marks.len().saturating_sub(1);
        self.notice = format!("Marked pane {} ({} mark(s))", pane_id, self.marks.len());
        self.view = View::Notice;
        show_self(true);
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
        if let Some(mark) = self.marks.get_mut(self.selected) {
            mark.name = if name.is_empty() { None } else { Some(name) };
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
            self.notice = format!("Pane {} no longer exists", mark.pane_id);
            self.view = View::Notice;
            set_timeout(1.5);
            return;
        };

        let mut target_top = mark.top_offset;

        if !mark.anchor.is_empty() {
            if let Ok(contents) = get_pane_scrollback(pane_id, true) {
                let mut lines = contents.lines_above_viewport;
                lines.extend(contents.viewport);
                lines.extend(contents.lines_below_viewport);

                let original_anchor_index = mark.top_offset + mark.cursor_row;
                if let Some((match_index, _)) = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| **line == mark.anchor)
                    .min_by_key(|(index, _)| index.abs_diff(original_anchor_index))
                {
                    target_top = match_index.saturating_sub(mark.cursor_row);
                }
            }
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
                        self.marks.remove(self.selected);
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
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadPaneContents,
            PermissionType::ChangeApplicationState,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;
                self.run_pending();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                self.pending_action = None;
                false
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
            if self.permissions_granted {
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
                println!("zmark — session bookmarks");
                println!();

                if self.marks.is_empty() {
                    println!("No marks yet.");
                } else {
                    let available = rows.saturating_sub(5).max(1);
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
