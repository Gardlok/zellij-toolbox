use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
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

#[derive(Clone)]
struct PendingJump {
    pane_id: u32,
    target_top: usize,
    page_steps_remaining: usize,
    correction_steps_remaining: usize,
    correction_direction: CorrectionDirection,
    query: String,
    case_sensitive: bool,
    request: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CorrectionDirection {
    None,
    Up,
    Down,
}

impl Default for CorrectionDirection {
    fn default() -> Self {
        Self::None
    }
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
    jump_request: u64,
    pending_jump: Option<PendingJump>,
    highlighted_pane: Option<u32>,
}

impl State {
    fn open(&mut self) {
        self.clear_previous_highlight();
        self.pending_open = false;
        self.pending_jump = None;
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

    fn resolve_target_top(&self, result: &SearchResult) -> Option<usize> {
        let pane_id = PaneId::Terminal(result.pane_id);
        let contents = get_pane_scrollback(pane_id, true).ok()?;

        let mut lines = contents.lines_above_viewport;
        lines.extend(contents.viewport);
        lines.extend(contents.lines_below_viewport);

        if lines.is_empty() {
            return None;
        }

        Some(
            lines
                .iter()
                .enumerate()
                .filter(|(_, line)| **line == result.text)
                .min_by_key(|(index, _)| index.abs_diff(result.line_number))
                .map(|(index, _)| index)
                .unwrap_or(result.line_number.min(lines.len() - 1)),
        )
    }

    fn regex_escape_literal(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len());
        for c in value.chars() {
            if matches!(
                c,
                '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|'
            ) {
                escaped.push('\\');
            }
            escaped.push(c);
        }
        escaped
    }

    fn highlight_pattern(query: &str, case_sensitive: bool) -> Option<String> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }

        let escaped = Self::regex_escape_literal(query);
        if case_sensitive {
            Some(escaped)
        } else {
            Some(format!("(?i:{escaped})"))
        }
    }

    fn clear_previous_highlight(&mut self) {
        if let Some(pane_id) = self.highlighted_pane.take() {
            clear_pane_highlights(PaneId::Terminal(pane_id));
        }
    }

    fn apply_highlight(&mut self, pane_id: u32, query: &str, case_sensitive: bool) {
        self.clear_previous_highlight();

        let Some(pattern) = Self::highlight_pattern(query, case_sensitive) else {
            return;
        };

        set_pane_regex_highlights(
            PaneId::Terminal(pane_id),
            vec![RegexHighlight {
                pattern,
                style: HighlightStyle::BackgroundEmphasis0,
                layer: HighlightLayer::ActionFeedback,
                context: BTreeMap::new(),
                on_hover: false,
                bold: true,
                italic: false,
                underline: false,
                tooltip_text: None,
            }],
        );
        self.highlighted_pane = Some(pane_id);
    }

    fn jump_context(request: u64, stage: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("zgrep-op".to_owned(), "jump".to_owned()),
            ("zgrep-request".to_owned(), request.to_string()),
            ("zgrep-stage".to_owned(), stage.to_owned()),
        ])
    }

    fn run_jump_action(&self, action: Action, request: u64, stage: &str) {
        run_action(action, Self::jump_context(request, stage));
    }

    fn start_jump(&mut self) {
        let Some(result) = self.results.get(self.selected).cloned() else {
            return;
        };

        let Some(target_top) = self.resolve_target_top(&result) else {
            self.status = "Could not re-resolve that result in current scrollback.".to_owned();
            return;
        };

        self.jump_request = self.jump_request.saturating_add(1);
        let request = self.jump_request;

        self.pending_jump = Some(PendingJump {
            pane_id: result.pane_id,
            target_top,
            page_steps_remaining: target_top / result.pane_rows,
            correction_steps_remaining: 0,
            correction_direction: CorrectionDirection::None,
            query: self.query.trim().to_owned(),
            case_sensitive: self.case_sensitive,
            request,
        });

        self.visible = false;
        hide_self();

        self.run_jump_action(
            Action::FocusTerminalPaneWithId {
                pane_id: result.pane_id,
                should_float_if_hidden: false,
                should_be_in_place_if_hidden: false,
            },
            request,
            "focus",
        );
    }

    fn run_next_page_step(&self, request: u64) {
        self.run_jump_action(Action::PageScrollDown, request, "page");
    }

    fn measure_and_begin_correction(&mut self, request: u64) {
        let Some(pending) = self.pending_jump.as_mut() else {
            return;
        };

        let pane_id = PaneId::Terminal(pending.pane_id);
        let actual_top = get_pane_scrollback(pane_id, true)
            .map(|contents| contents.lines_above_viewport.len())
            .unwrap_or(pending.target_top);

        if actual_top < pending.target_top {
            pending.correction_direction = CorrectionDirection::Down;
            pending.correction_steps_remaining = pending.target_top - actual_top;
        } else if actual_top > pending.target_top {
            pending.correction_direction = CorrectionDirection::Up;
            pending.correction_steps_remaining = actual_top - pending.target_top;
        } else {
            pending.correction_direction = CorrectionDirection::None;
            pending.correction_steps_remaining = 0;
        }

        self.run_next_correction_step_or_finish(request);
    }

    fn run_next_correction_step_or_finish(&mut self, request: u64) {
        let Some(pending) = self.pending_jump.as_ref() else {
            return;
        };

        if pending.correction_steps_remaining == 0 {
            let pane_id = pending.pane_id;
            let query = pending.query.clone();
            let case_sensitive = pending.case_sensitive;
            self.pending_jump = None;
            self.apply_highlight(pane_id, &query, case_sensitive);
            return;
        }

        let action = match pending.correction_direction {
            CorrectionDirection::Up => Action::ScrollUp,
            CorrectionDirection::Down => Action::ScrollDown,
            CorrectionDirection::None => return,
        };
        self.run_jump_action(action, request, "correct");
    }

    fn handle_jump_action_complete(&mut self, context: BTreeMap<String, String>) -> bool {
        if context.get("zgrep-op").map(String::as_str) != Some("jump") {
            return false;
        }

        let Some(request) = context
            .get("zgrep-request")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return false;
        };

        let Some(stage) = context.get("zgrep-stage").cloned() else {
            return false;
        };

        let Some(pending) = self.pending_jump.as_ref() else {
            return false;
        };
        if pending.request != request {
            return false;
        }

        match stage.as_str() {
            "focus" => {
                self.run_jump_action(Action::ScrollToTop, request, "top");
            }
            "top" => {
                let page_steps = self
                    .pending_jump
                    .as_ref()
                    .map(|pending| pending.page_steps_remaining)
                    .unwrap_or(0);
                if page_steps > 0 {
                    self.run_next_page_step(request);
                } else {
                    self.measure_and_begin_correction(request);
                }
            }
            "page" => {
                let remaining = if let Some(pending) = self.pending_jump.as_mut() {
                    pending.page_steps_remaining = pending.page_steps_remaining.saturating_sub(1);
                    pending.page_steps_remaining
                } else {
                    0
                };

                if remaining > 0 {
                    self.run_next_page_step(request);
                } else {
                    self.measure_and_begin_correction(request);
                }
            }
            "correct" => {
                if let Some(pending) = self.pending_jump.as_mut() {
                    pending.correction_steps_remaining =
                        pending.correction_steps_remaining.saturating_sub(1);
                }
                self.run_next_correction_step_or_finish(request);
            }
            _ => {}
        }

        false
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
                BareKey::Enter => self.start_jump(),
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
            EventType::ActionComplete,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadPaneContents,
            PermissionType::ChangeApplicationState,
            PermissionType::RunActionsAsUser,
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
                self.pending_jump = None;
                false
            }
            Event::PaneUpdate(manifest) => {
                self.pane_manifest = manifest;
                self.visible
            }
            Event::ActionComplete(_action, _pane_id, context) => {
                self.handle_jump_action_complete(context)
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
                    "Up/Down: select   Enter: exact jump/highlight   c: case   s/Tab: scope   /: edit   Esc: close"
                );
            }
        }
    }
}
