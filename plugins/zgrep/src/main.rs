use std::collections::BTreeMap;
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Clone)]
struct SearchResult {
    pane_id: u32,
    tab_index: usize,
    title: String,
    line_number: usize,
    pane_rows: usize,
    text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Query,
    Results,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Query
    }
}

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    pane_manifest: PaneManifest,
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    mode: Mode,
    status: String,
}

impl State {
    fn open(&mut self) {
        self.pending_open = false;
        self.visible = true;
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.mode = Mode::Query;
        self.status = "Type a search term and press Enter.".to_owned();
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }

    fn search(&mut self) {
        let needle = self.query.trim().to_lowercase();
        self.results.clear();
        self.selected = 0;

        if needle.is_empty() {
            self.status = "Search term is empty.".to_owned();
            return;
        }

        let mut tab_indexes: Vec<usize> = self.pane_manifest.panes.keys().copied().collect();
        tab_indexes.sort_unstable();

        const MAX_RESULTS: usize = 200;

        for tab_index in tab_indexes {
            let Some(panes) = self.pane_manifest.panes.get(&tab_index) else {
                continue;
            };

            for pane in panes {
                if pane.is_plugin || !pane.is_selectable {
                    continue;
                }

                let pane_id = PaneId::Terminal(pane.id);
                let Ok(contents) = get_pane_scrollback(pane_id, true) else {
                    continue;
                };

                let mut lines = contents.lines_above_viewport;
                lines.extend(contents.viewport);
                lines.extend(contents.lines_below_viewport);

                for (line_number, line) in lines.into_iter().enumerate() {
                    if line.to_lowercase().contains(&needle) {
                        self.results.push(SearchResult {
                            pane_id: pane.id,
                            tab_index,
                            title: pane.title.clone(),
                            line_number,
                            pane_rows: pane.pane_content_rows.max(1),
                            text: line,
                        });

                        if self.results.len() >= MAX_RESULTS {
                            break;
                        }
                    }
                }

                if self.results.len() >= MAX_RESULTS {
                    break;
                }
            }

            if self.results.len() >= MAX_RESULTS {
                break;
            }
        }

        self.mode = Mode::Results;
        self.status = if self.results.is_empty() {
            "No matches. Press / to search again.".to_owned()
        } else if self.results.len() >= MAX_RESULTS {
            format!("Showing first {} matches.", MAX_RESULTS)
        } else {
            format!("{} match(es).", self.results.len())
        };
    }

    fn jump_to_selected(&mut self) {
        let Some(result) = self.results.get(self.selected).cloned() else {
            return;
        };

        let pane_id = PaneId::Terminal(result.pane_id);
        self.visible = false;
        hide_self();

        focus_terminal_pane(result.pane_id, false, false);
        scroll_to_top_in_pane_id(pane_id);

        let page_size = result.pane_rows.saturating_sub(1).max(1);
        let page_count = result.line_number / page_size;
        let remainder = result.line_number % page_size;

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

        match self.mode {
            Mode::Query => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Enter => self.search(),
                BareKey::Backspace => {
                    self.query.pop();
                }
                BareKey::Char(c) => self.query.push(c),
                _ => {}
            },
            Mode::Results => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.results.is_empty() {
                        self.selected = (self.selected + 1).min(self.results.len() - 1);
                    }
                }
                BareKey::Enter => self.jump_to_selected(),
                BareKey::Char('/') => {
                    self.mode = Mode::Query;
                    self.results.clear();
                    self.selected = 0;
                    self.status = "Edit the query and press Enter.".to_owned();
                }
                _ => {}
            },
        }

        true
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::PaneUpdate,
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
                self.visible
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
        println!("zgrep");
        println!();

        match self.mode {
            Mode::Query => {
                println!("Search: {}", self.query);
                println!();
                println!("{}", self.status);
                println!();
                println!("Enter: search    Esc: close");
            }
            Mode::Results => {
                println!("Search: {}", self.query);
                println!("{}", self.status);
                println!();

                let available = rows.saturating_sub(7).max(1);
                let start = self.selected.saturating_sub(available.saturating_sub(1));
                let end = (start + available).min(self.results.len());

                for (index, result) in self.results[start..end].iter().enumerate() {
                    let absolute_index = start + index;
                    let marker = if absolute_index == self.selected {
                        ">"
                    } else {
                        " "
                    };
                    let prefix = format!(
                        "{} T{} P{} {}:{} ",
                        marker,
                        result.tab_index + 1,
                        result.pane_id,
                        result.title,
                        result.line_number + 1
                    );
                    let width = cols.saturating_sub(prefix.chars().count());
                    let text: String = result.text.chars().take(width).collect();
                    println!("{}{}", prefix, text);
                }

                println!();
                println!("Up/Down: select   Enter: jump   /: new search   Esc: close");
            }
        }
    }
}
