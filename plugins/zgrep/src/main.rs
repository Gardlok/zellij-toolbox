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
    Input,
    List,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Input
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
    results_query: String,
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
    dialog_request: u64,
    plugin_id: Option<u32>,
    dialog_is_floating: bool,
    floating_preference: Option<bool>,
    pending_float_resize: bool,
}

impl State {
    fn open(&mut self) {
        self.pending_open = false;
        self.pending_jump = None;
        self.visible = true;

        if self.query.is_empty() && self.results.is_empty() {
            self.selected = 0;
            self.mode = Mode::Input;
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
        } else if self.results.is_empty() || self.query.trim() != self.results_query {
            self.mode = Mode::Input;
            self.status = "Resume the query and press Enter.".to_owned();
        } else {
            self.mode = Mode::List;
            self.selected = self.selected.min(self.results.len().saturating_sub(1));
            self.status = format!("{} buffered match(es).", self.results.len());
        }

        let should_float = self.floating_preference.unwrap_or(true);
        self.pending_float_resize = should_float;
        switch_to_input_mode(&InputMode::Normal);
        show_self(should_float);
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
        self.clear_previous_highlight();

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

        self.results_query = needle;
        if self.results.is_empty() {
            self.mode = Mode::Input;
            self.status = "No matches. Edit the query and press Enter.".to_owned();
        } else {
            self.mode = Mode::List;
            self.status = if self.results.len() >= MAX_RESULTS {
                format!("Showing first {} matches.", MAX_RESULTS)
            } else {
                format!("{} match(es).", self.results.len())
            };
        }
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

        let selected_index = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| **line == result.text)
            .min_by_key(|(index, _)| index.abs_diff(result.line_number))
            .map(|(index, _)| index)
            .unwrap_or(result.line_number.min(lines.len() - 1));

        Some(selected_index.saturating_sub(result.pane_rows / 2))
    }

    fn regex_escape_literal(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len() + 8);
        for character in value.chars() {
            match character {
                '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^'
                | '
                _ => {}
            }
            escaped.push(character);
        }
        escaped
    }

    fn highlight_pattern(query: &str, case_sensitive: bool) -> Option<String> {
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
                style: HighlightStyle::Emphasis0,
                layer: HighlightLayer::ActionFeedback,
                context: BTreeMap::new(),
                on_hover: false,
                bold: true,
                italic: false,
                underline: true,
                tooltip_text: None,
            }],
        );
        self.highlighted_pane = Some(pane_id);
    }
    fn own_pane_id(&self) -> Option<PaneId> {
        self.plugin_id.map(PaneId::Plugin)
    }

    fn resize_floating_dialog(&mut self) {
        let Some(pane_id) = self.own_pane_id() else {
            return;
        };

        let coordinates = FloatingPaneCoordinates::default()
            .with_x_percent(10)
            .with_y_percent(10)
            .with_width_percent(80)
            .with_height_percent(80);
        change_floating_panes_coordinates(vec![(pane_id, coordinates)]);
        self.pending_float_resize = false;
    }

    fn refresh_dialog_state(&mut self) {
        let Some(plugin_id) = self.plugin_id else {
            return;
        };

        let pane_info = self
            .pane_manifest
            .panes
            .values()
            .flatten()
            .find(|pane| pane.is_plugin && pane.id == plugin_id);

        if let Some(pane_info) = pane_info {
            self.dialog_is_floating = pane_info.is_floating;
            if self.visible && self.dialog_is_floating && self.pending_float_resize {
                self.resize_floating_dialog();
            }
        }
    }

    fn dialog_context(
        request: u64,
        target_floating: bool,
        stage: &str,
    ) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("zgrep-op".to_owned(), "dialog".to_owned()),
            ("zgrep-request".to_owned(), request.to_string()),
            (
                "zgrep-target-floating".to_owned(),
                target_floating.to_string(),
            ),
            ("zgrep-stage".to_owned(), stage.to_owned()),
        ])
    }

    fn run_dialog_action(&self, action: Action, request: u64, target_floating: bool, stage: &str) {
        run_action(
            action,
            Self::dialog_context(request, target_floating, stage),
        );
    }

    fn toggle_dialog_layer(&mut self) {
        let Some(_plugin_id) = self.plugin_id else {
            self.status = "Could not identify the zgrep pane.".to_owned();
            return;
        };

        let target_floating = !self.dialog_is_floating;
        self.dialog_request = self.dialog_request.saturating_add(1);
        let request = self.dialog_request;

        self.floating_preference = Some(target_floating);
        self.pending_float_resize = false;
        self.run_dialog_action(
            Action::TogglePaneEmbedOrFloating,
            request,
            target_floating,
            "toggle",
        );
    }

    fn handle_dialog_action_complete(&mut self, context: BTreeMap<String, String>) -> bool {
        if context.get("zgrep-op").map(String::as_str) != Some("dialog") {
            return false;
        }

        let Some(request) = context
            .get("zgrep-request")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return false;
        };
        if request != self.dialog_request {
            return false;
        }

        let target_floating =
            context.get("zgrep-target-floating").map(String::as_str) == Some("true");
        let Some(stage) = context.get("zgrep-stage").map(String::as_str) else {
            return false;
        };
        let Some(plugin_id) = self.plugin_id else {
            return false;
        };

        match stage {
            "toggle" => {
                self.run_dialog_action(
                    Action::FocusPluginPaneWithId {
                        pane_id: plugin_id,
                        should_float_if_hidden: target_floating,
                        should_be_in_place_if_hidden: false,
                    },
                    request,
                    target_floating,
                    "focus",
                );
            }
            "focus" if target_floating => {
                let coordinates = FloatingPaneCoordinates::default()
                    .with_x_percent(10)
                    .with_y_percent(10)
                    .with_width_percent(80)
                    .with_height_percent(80);
                self.run_dialog_action(
                    Action::ChangeFloatingPaneCoordinates {
                        pane_id: PaneId::Plugin(plugin_id),
                        coordinates,
                    },
                    request,
                    target_floating,
                    "resize",
                );
            }
            "focus" | "resize" => {}
            _ => {}
        }

        self.visible
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

        let query = self.query.trim().to_owned();
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
            query,
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
            self.run_jump_action(
                Action::SwitchToMode {
                    input_mode: InputMode::Scroll,
                },
                request,
                "scroll-mode",
            );
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
            "scroll-mode" => {
                if let Some(pending) = self.pending_jump.take() {
                    self.apply_highlight(pending.pane_id, &pending.query, pending.case_sensitive);
                }
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

    fn edit_query(&mut self) {
        self.mode = Mode::Input;
        self.status = "Edit the query and press Enter to refresh results.".to_owned();
    }

    fn reset_search(&mut self) {
        self.clear_previous_highlight();
        self.query.clear();
        self.results.clear();
        self.results_query.clear();
        self.selected = 0;
        self.mode = Mode::Input;
        self.case_sensitive = false;
        self.scope = Scope::All;
        self.status = "Search cleared. Type a new term and press Enter.".to_owned();
    }

    fn select_next_result(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }
    }

    fn select_previous_result(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn select_adjacent_result_in_pane(&mut self, pane_id: u32, forward: bool) -> bool {
        let matching: Vec<usize> = self
            .results
            .iter()
            .enumerate()
            .filter_map(|(index, result)| (result.pane_id == pane_id).then_some(index))
            .collect();
        if matching.is_empty() {
            return false;
        }

        let next_index = match matching.iter().position(|index| *index == self.selected) {
            Some(position) if forward => matching[(position + 1) % matching.len()],
            Some(position) => matching[(position + matching.len() - 1) % matching.len()],
            None if forward => matching[0],
            None => *matching.last().unwrap_or(&matching[0]),
        };
        self.selected = next_index;
        true
    }

    fn jump_adjacent_result_in_current_pane(&mut self, forward: bool) {
        let Ok((_tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            return;
        };
        if self.select_adjacent_result_in_pane(pane_id, forward) {
            self.start_jump();
        }
    }
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if key.bare_key == BareKey::Char('f') && key.has_modifiers(&[KeyModifier::Ctrl]) {
            self.toggle_dialog_layer();
            return true;
        }

        if !key.has_no_modifiers() {
            return true;
        }

        match self.mode {
            Mode::Input => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Enter => self.search(),
                BareKey::Tab => {
                    if !self.results.is_empty() && self.query.trim() == self.results_query {
                        self.mode = Mode::List;
                        self.status = format!("{} buffered match(es).", self.results.len());
                    } else {
                        self.status = "Press Enter to search the edited query.".to_owned();
                    }
                }
                BareKey::Backspace => {
                    self.query.pop();
                }
                BareKey::Char(c) => self.query.push(c),
                _ => {}
            },
            Mode::List => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') | BareKey::Char('p') => {
                    self.select_previous_result();
                }
                BareKey::Down | BareKey::Char('j') | BareKey::Char('n') => {
                    self.select_next_result();
                }
                BareKey::Enter => self.start_jump(),
                BareKey::Tab | BareKey::Char('/') | BareKey::Char('e') => {
                    self.edit_query();
                }
                BareKey::Char('r') => {
                    self.reset_search();
                }
                BareKey::Char('c') => {
                    self.toggle_case();
                    self.search();
                }
                BareKey::Char('s') => {
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
        self.plugin_id = Some(get_plugin_ids().plugin_id);

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
                self.refresh_dialog_state();
                self.visible
            }
            Event::ActionComplete(_action, _pane_id, context) => {
                if context.get("zgrep-op").map(String::as_str) == Some("dialog") {
                    self.handle_dialog_action_complete(context)
                } else {
                    self.handle_jump_action_complete(context)
                }
            }
            Event::Key(key) if self.visible => self.handle_key(key),
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        match pipe_message.name.as_str() {
            "open" => {
                self.request_open();
                true
            }
            "next" => {
                self.jump_adjacent_result_in_current_pane(true);
                true
            }
            "previous" => {
                self.jump_adjacent_result_in_current_pane(false);
                true
            }
            _ => false,
        }
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
            Mode::Input => {
                println!("Input");
                println!();
                println!("Search: {}", self.query);
                println!("Scope: {}   Case: {}", self.scope.label(), case);
                println!();
                println!("{}", self.status);
                println!();
                println!("Enter: search   Tab: buffered list   Ctrl+F: float/dock   Esc: close");
            }
            Mode::List => {
                println!("List");
                println!();

                let view = if self.dialog_is_floating {
                    "floating"
                } else {
                    "docked"
                };

                if self.dialog_is_floating {
                    println!(
                        "{} | {} | {} | {}",
                        self.query,
                        self.scope.label(),
                        case,
                        view
                    );
                    println!("{}", self.status);
                } else {
                    println!("Search: {}", self.query);
                    println!(
                        "Scope: {}   Case: {}   View: {}",
                        self.scope.label(),
                        case,
                        view
                    );
                    println!("{}", self.status);
                    println!();
                }

                let reserved_rows = if self.dialog_is_floating { 6 } else { 8 };
                let available = rows.saturating_sub(reserved_rows).max(1);
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
                    "Up/Down n/p: select  Enter: jump  Tab/e: input  c: case  s: scope  r: reset  Ctrl+F: float/dock  Esc: close"
                );
            }
        }
    }
}
 => escaped.push('\\'),
                _ => {}
            }
            escaped.push(character);
        }
        escaped
    }

    fn highlight_pattern(query: &str, case_sensitive: bool) -> Option<String> {
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
                style: HighlightStyle::Emphasis0,
                layer: HighlightLayer::ActionFeedback,
                context: BTreeMap::new(),
                on_hover: false,
                bold: true,
                italic: false,
                underline: true,
                tooltip_text: None,
            }],
        );
        self.highlighted_pane = Some(pane_id);
    }
    fn own_pane_id(&self) -> Option<PaneId> {
        self.plugin_id.map(PaneId::Plugin)
    }

    fn resize_floating_dialog(&mut self) {
        let Some(pane_id) = self.own_pane_id() else {
            return;
        };

        let coordinates = FloatingPaneCoordinates::default()
            .with_x_percent(10)
            .with_y_percent(10)
            .with_width_percent(80)
            .with_height_percent(80);
        change_floating_panes_coordinates(vec![(pane_id, coordinates)]);
        self.pending_float_resize = false;
    }

    fn refresh_dialog_state(&mut self) {
        let Some(plugin_id) = self.plugin_id else {
            return;
        };

        let pane_info = self
            .pane_manifest
            .panes
            .values()
            .flatten()
            .find(|pane| pane.is_plugin && pane.id == plugin_id);

        if let Some(pane_info) = pane_info {
            self.dialog_is_floating = pane_info.is_floating;
            if self.visible && self.dialog_is_floating && self.pending_float_resize {
                self.resize_floating_dialog();
            }
        }
    }

    fn dialog_context(
        request: u64,
        target_floating: bool,
        stage: &str,
    ) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("zgrep-op".to_owned(), "dialog".to_owned()),
            ("zgrep-request".to_owned(), request.to_string()),
            (
                "zgrep-target-floating".to_owned(),
                target_floating.to_string(),
            ),
            ("zgrep-stage".to_owned(), stage.to_owned()),
        ])
    }

    fn run_dialog_action(&self, action: Action, request: u64, target_floating: bool, stage: &str) {
        run_action(
            action,
            Self::dialog_context(request, target_floating, stage),
        );
    }

    fn toggle_dialog_layer(&mut self) {
        let Some(_plugin_id) = self.plugin_id else {
            self.status = "Could not identify the zgrep pane.".to_owned();
            return;
        };

        let target_floating = !self.dialog_is_floating;
        self.dialog_request = self.dialog_request.saturating_add(1);
        let request = self.dialog_request;

        self.floating_preference = Some(target_floating);
        self.pending_float_resize = false;
        self.run_dialog_action(
            Action::TogglePaneEmbedOrFloating,
            request,
            target_floating,
            "toggle",
        );
    }

    fn handle_dialog_action_complete(&mut self, context: BTreeMap<String, String>) -> bool {
        if context.get("zgrep-op").map(String::as_str) != Some("dialog") {
            return false;
        }

        let Some(request) = context
            .get("zgrep-request")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return false;
        };
        if request != self.dialog_request {
            return false;
        }

        let target_floating =
            context.get("zgrep-target-floating").map(String::as_str) == Some("true");
        let Some(stage) = context.get("zgrep-stage").map(String::as_str) else {
            return false;
        };
        let Some(plugin_id) = self.plugin_id else {
            return false;
        };

        match stage {
            "toggle" => {
                self.run_dialog_action(
                    Action::FocusPluginPaneWithId {
                        pane_id: plugin_id,
                        should_float_if_hidden: target_floating,
                        should_be_in_place_if_hidden: false,
                    },
                    request,
                    target_floating,
                    "focus",
                );
            }
            "focus" if target_floating => {
                let coordinates = FloatingPaneCoordinates::default()
                    .with_x_percent(10)
                    .with_y_percent(10)
                    .with_width_percent(80)
                    .with_height_percent(80);
                self.run_dialog_action(
                    Action::ChangeFloatingPaneCoordinates {
                        pane_id: PaneId::Plugin(plugin_id),
                        coordinates,
                    },
                    request,
                    target_floating,
                    "resize",
                );
            }
            "focus" | "resize" => {}
            _ => {}
        }

        self.visible
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

        let query = self.query.trim().to_owned();
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
            query,
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
            self.run_jump_action(
                Action::SwitchToMode {
                    input_mode: InputMode::Scroll,
                },
                request,
                "scroll-mode",
            );
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
            "scroll-mode" => {
                if let Some(pending) = self.pending_jump.take() {
                    self.apply_highlight(
                        pending.pane_id,
                        &pending.query,
                        pending.case_sensitive,
                    );
                }
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

    fn edit_query(&mut self) {
        self.mode = Mode::Input;
        self.status = "Edit the query and press Enter to refresh results.".to_owned();
    }

    fn reset_search(&mut self) {
        self.clear_previous_highlight();
        self.query.clear();
        self.results.clear();
        self.results_query.clear();
        self.selected = 0;
        self.mode = Mode::Input;
        self.case_sensitive = false;
        self.scope = Scope::All;
        self.status = "Search cleared. Type a new term and press Enter.".to_owned();
    }

    fn select_next_result(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }
    }

    fn select_previous_result(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn select_adjacent_result_in_pane(&mut self, pane_id: u32, forward: bool) -> bool {
        let matching: Vec<usize> = self
            .results
            .iter()
            .enumerate()
            .filter_map(|(index, result)| (result.pane_id == pane_id).then_some(index))
            .collect();
        if matching.is_empty() {
            return false;
        }

        let next_index = match matching.iter().position(|index| *index == self.selected) {
            Some(position) if forward => matching[(position + 1) % matching.len()],
            Some(position) => matching[(position + matching.len() - 1) % matching.len()],
            None if forward => matching[0],
            None => *matching.last().unwrap_or(&matching[0]),
        };
        self.selected = next_index;
        true
    }

    fn jump_adjacent_result_in_current_pane(&mut self, forward: bool) {
        let Ok((_tab_index, PaneId::Terminal(pane_id))) = get_focused_pane_info() else {
            return;
        };
        if self.select_adjacent_result_in_pane(pane_id, forward) {
            self.start_jump();
        }
    }
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if key.bare_key == BareKey::Char('f') && key.has_modifiers(&[KeyModifier::Ctrl]) {
            self.toggle_dialog_layer();
            return true;
        }

        if !key.has_no_modifiers() {
            return true;
        }

        match self.mode {
            Mode::Input => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Enter => self.search(),
                BareKey::Tab => {
                    if !self.results.is_empty() && self.query.trim() == self.results_query {
                        self.mode = Mode::List;
                        self.status = format!("{} buffered match(es).", self.results.len());
                    } else {
                        self.status = "Press Enter to search the edited query.".to_owned();
                    }
                }
                BareKey::Backspace => {
                    self.query.pop();
                }
                BareKey::Char(c) => self.query.push(c),
                _ => {}
            },
            Mode::List => match key.bare_key {
                BareKey::Esc => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') | BareKey::Char('p') => {
                    self.select_previous_result();
                }
                BareKey::Down | BareKey::Char('j') | BareKey::Char('n') => {
                    self.select_next_result();
                }
                BareKey::Enter => self.start_jump(),
                BareKey::Tab | BareKey::Char('/') | BareKey::Char('e') => {
                    self.edit_query();
                }
                BareKey::Char('r') => {
                    self.reset_search();
                }
                BareKey::Char('c') => {
                    self.toggle_case();
                    self.search();
                }
                BareKey::Char('s') => {
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
        self.plugin_id = Some(get_plugin_ids().plugin_id);

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
                self.refresh_dialog_state();
                self.visible
            }
            Event::ActionComplete(_action, _pane_id, context) => {
                if context.get("zgrep-op").map(String::as_str) == Some("dialog") {
                    self.handle_dialog_action_complete(context)
                } else {
                    self.handle_jump_action_complete(context)
                }
            }
            Event::Key(key) if self.visible => self.handle_key(key),
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        match pipe_message.name.as_str() {
            "open" => {
                self.request_open();
                true
            }
            "next" => {
                self.jump_adjacent_result_in_current_pane(true);
                true
            }
            "previous" => {
                self.jump_adjacent_result_in_current_pane(false);
                true
            }
            _ => false,
        }
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
            Mode::Input => {
                println!("Input");
                println!();
                println!("Search: {}", self.query);
                println!("Scope: {}   Case: {}", self.scope.label(), case);
                println!();
                println!("{}", self.status);
                println!();
                println!("Enter: search   Tab: buffered list   Ctrl+F: float/dock   Esc: close");
            }
            Mode::List => {
                println!("List");
                println!();

                let view = if self.dialog_is_floating {
                    "floating"
                } else {
                    "docked"
                };

                if self.dialog_is_floating {
                    println!(
                        "{} | {} | {} | {}",
                        self.query,
                        self.scope.label(),
                        case,
                        view
                    );
                    println!("{}", self.status);
                } else {
                    println!("Search: {}", self.query);
                    println!(
                        "Scope: {}   Case: {}   View: {}",
                        self.scope.label(),
                        case,
                        view
                    );
                    println!("{}", self.status);
                    println!();
                }

                let reserved_rows = if self.dialog_is_floating { 6 } else { 8 };
                let available = rows.saturating_sub(reserved_rows).max(1);
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
                    "Up/Down n/p: select  Enter: jump  Tab/e: input  c: case  s: scope  r: reset  Ctrl+F: float/dock  Esc: close"
                );
            }
        }
    }
}
