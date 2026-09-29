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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    All,
    CurrentTab,
    FocusedPane,
}

impl Default for Scope {
    fn default() -> Self {
        Self::All
    }
}

impl Scope {
    fn next(self) -> Self {
        match self {
            Self::All => Self::CurrentTab,
            Self::CurrentTab => Self::FocusedPane,
            Self::FocusedPane => Self::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "all panes",
            Self::CurrentTab => "current tab",
            Self::FocusedPane => "focused pane",
        }
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
    case_sensitive: bool,
    scope: Scope,
    origin_tab: Option<usize>,
    origin_pane: Option<u32>,
}

impl State {
    fn open(&mut self) {
        self.pending_open = false;
        self.visible = true;
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.mode = Mode::Query;
        self.case_sensitive = false;
        self.scope = Scope::All;

        match get_focused_pane_info() {
            Ok((tab_index, PaneId::Terminal(pane_id))) => {
                self.origin_tab = Some(tab_index);
                self.origin_pane = Some(pane_id);
            }
            Ok((tab_index, _)) => {
                self.origin_tab = Some(tab_index);
                self.origin_pane = None;
            }
            Err(_) => {
                self.origin_tab = None;
                self.origin_pane = None;
            }
        }

        self.status = "Type a search term and press Enter.".to_owned();
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }

    fn pane_in_scope(&self, tab_index: usize, pane_id: u32) -> bool {
        match self.scope {
            Scope::All => true,
            Scope::CurrentTab => self.origin_tab == Some(tab_index),
            Scope::FocusedPane => self.origin_pane == Some(pane_id),
        }
    }

    fn line_matches(&self, line: &str, needle: &str, folded_needle: &str) -> bool {
        if self.case_sensitive {
            line.contains(needle)
        } else {
            line.to_lowercase().contains(folded_needle)
        }
    }

    fn search(&mut self) {
        let needle = self.query.trim().to_owned();
        let folded_needle = needle.to_lowercase();
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
                if pane.is_plugin || !pane.is_selectable || !self.pane_in_scope(tab_index, pane.id)
                {
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
                    if self.line_matches(&line, &needle, &folded_needle) {
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
            "No matches. Press / to edit the search.".to_owned()
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

    fn cycle_scope(&mut self) {
        self.scope = self.scope.next();
        self.status = format!("Scope: {}.", self.scope.label());
    }

    fn toggle_case(&mut self) {
        self.case_sensitive = !self.case_sensitive;
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
                BareKey::Tab => self.cycle_scope(),
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
                BareKey::Char('c') => {
                    self.toggle_case();
                    self.search();
                }
                BareKey::Char('s') | BareKey::Tab => {
                    self.cycle_scope();
                    self.search();
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

        let case = if self.case_sensitive {
            "sensitive"
        } else {
            "insensitive"
        };

        match self.mode {
            Mode::Query => {
                println!("Search: {}", self.query);
                println!("Scope: {}   Case: {}", self.scope.label(), case);
                println!();
                println!("{}", self.status);
                println!();
                println!("Tab: scope   Enter: search   Esc: close");
            }
            Mode::Results => {
                println!("Search: {}", self.query);
                println!("Scope: {}   Case: {}", self.scope.label(), case);
                println!("{}", self.status);
                println!();

                let available = rows.saturating_sub(8).max(1);
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
                println!(
                    "Up/Down: select   Enter: jump   c: case   s/Tab: scope   /: edit   Esc: close"
                );
            }
        }
    }
}
