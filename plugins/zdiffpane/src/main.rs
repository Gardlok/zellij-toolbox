use std::collections::BTreeMap;
use zellij_tile::prelude::*;

register_plugin!(State);

const MAX_LINES_PER_PANE: usize = 300;

#[derive(Clone)]
struct PaneChoice {
    pane_id: u32,
    tab_index: usize,
    title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    PickA,
    PickB,
    Diff,
}

impl Default for Step {
    fn default() -> Self {
        Self::PickA
    }
}

#[derive(Default)]
struct State {
    permissions_granted: bool,
    pending_open: bool,
    visible: bool,
    pane_manifest: PaneManifest,
    choices: Vec<PaneChoice>,
    selected: usize,
    pane_a: Option<PaneChoice>,
    pane_b: Option<PaneChoice>,
    step: Step,
    diff: Vec<String>,
    diff_offset: usize,
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

        self.selected = self.selected.min(self.choices.len().saturating_sub(1));
    }

    fn open(&mut self) {
        self.pending_open = false;
        self.visible = true;
        self.step = Step::PickA;
        self.pane_a = None;
        self.pane_b = None;
        self.diff.clear();
        self.diff_offset = 0;
        self.status = "Choose the first pane.".to_owned();
        self.rebuild_choices();
        show_self(true);
    }

    fn request_open(&mut self) {
        self.pending_open = true;
        if self.permissions_granted {
            self.open();
        }
    }

    fn pane_lines(pane_id: u32) -> Option<(Vec<String>, bool)> {
        let contents = get_pane_scrollback(PaneId::Terminal(pane_id), true).ok()?;
        let mut lines = contents.lines_above_viewport;
        lines.extend(contents.viewport);
        lines.extend(contents.lines_below_viewport);

        let truncated = lines.len() > MAX_LINES_PER_PANE;
        if truncated {
            lines = lines.split_off(lines.len() - MAX_LINES_PER_PANE);
        }

        Some((lines, truncated))
    }

    fn make_diff(a: &[String], b: &[String]) -> Vec<String> {
        let n = a.len();
        let m = b.len();
        let mut lcs = vec![vec![0usize; m + 1]; n + 1];

        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if a[i] == b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }

        let mut out = Vec::new();
        let mut i = 0;
        let mut j = 0;

        while i < n && j < m {
            if a[i] == b[j] {
                out.push(format!("  {}", a[i]));
                i += 1;
                j += 1;
            } else if lcs[i + 1][j] >= lcs[i][j + 1] {
                out.push(format!("- {}", a[i]));
                i += 1;
            } else {
                out.push(format!("+ {}", b[j]));
                j += 1;
            }
        }

        while i < n {
            out.push(format!("- {}", a[i]));
            i += 1;
        }
        while j < m {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }

        out
    }

    fn build_diff(&mut self) {
        let (Some(a), Some(b)) = (&self.pane_a, &self.pane_b) else {
            return;
        };

        let Some((a_lines, a_truncated)) = Self::pane_lines(a.pane_id) else {
            self.status = format!("Could not read pane {}", a.pane_id);
            return;
        };
        let Some((b_lines, b_truncated)) = Self::pane_lines(b.pane_id) else {
            self.status = format!("Could not read pane {}", b.pane_id);
            return;
        };

        self.diff = Self::make_diff(&a_lines, &b_lines);
        self.diff_offset = 0;
        self.step = Step::Diff;

        self.status = if a_truncated || b_truncated {
            format!(
                "Compared the most recent {} lines from each pane.",
                MAX_LINES_PER_PANE
            )
        } else {
            "Compared full retained scrollback for both panes.".to_owned()
        };
    }

    fn choose(&mut self) {
        let Some(choice) = self.choices.get(self.selected).cloned() else {
            return;
        };

        match self.step {
            Step::PickA => {
                self.pane_a = Some(choice);
                self.step = Step::PickB;
                self.status = "Choose the second pane.".to_owned();
            }
            Step::PickB => {
                if self
                    .pane_a
                    .as_ref()
                    .map(|a| a.pane_id == choice.pane_id)
                    .unwrap_or(false)
                {
                    self.status = "Choose a different second pane.".to_owned();
                } else {
                    self.pane_b = Some(choice);
                    self.build_diff();
                }
            }
            Step::Diff => {}
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if !key.has_no_modifiers() {
            return true;
        }

        match self.step {
            Step::PickA | Step::PickB => match key.bare_key {
                BareKey::Esc | BareKey::Char('q') => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    if !self.choices.is_empty() {
                        self.selected = (self.selected + 1).min(self.choices.len() - 1);
                    }
                }
                BareKey::Enter => self.choose(),
                BareKey::Char('f') => toggle_focus_no_ui_fullscreen(),
                _ => {}
            },
            Step::Diff => match key.bare_key {
                BareKey::Esc | BareKey::Char('q') => {
                    self.visible = false;
                    hide_self();
                }
                BareKey::Up | BareKey::Char('k') => {
                    self.diff_offset = self.diff_offset.saturating_sub(1);
                }
                BareKey::Down | BareKey::Char('j') => {
                    self.diff_offset =
                        (self.diff_offset + 1).min(self.diff.len().saturating_sub(1));
                }
                BareKey::PageUp => {
                    self.diff_offset = self.diff_offset.saturating_sub(20);
                }
                BareKey::PageDown => {
                    self.diff_offset =
                        (self.diff_offset + 20).min(self.diff.len().saturating_sub(1));
                }
                BareKey::Char('c') => {
                    copy_to_clipboard(self.diff.join("\n"));
                    self.status = "Diff copied to clipboard.".to_owned();
                }
                BareKey::Char('r') => {
                    self.step = Step::PickA;
                    self.pane_a = None;
                    self.pane_b = None;
                    self.diff.clear();
                    self.diff_offset = 0;
                    self.status = "Choose the first pane.".to_owned();
                }
                BareKey::Char('f') => toggle_focus_no_ui_fullscreen(),
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
            PermissionType::WriteToClipboard,
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
                if self.visible && self.step != Step::Diff {
                    self.rebuild_choices();
                    return true;
                }
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
        println!("zdiffpane");
        println!("{}", self.status);
        println!();

        match self.step {
            Step::PickA | Step::PickB => {
                let label = if self.step == Step::PickA {
                    "First pane"
                } else {
                    "Second pane"
                };
                println!("{}:", label);

                let available = rows.saturating_sub(6).max(1);
                let start = self.selected.saturating_sub(available.saturating_sub(1));
                let end = (start + available).min(self.choices.len());

                for (index, choice) in self.choices[start..end].iter().enumerate() {
                    let absolute_index = start + index;
                    let marker = if absolute_index == self.selected {
                        ">"
                    } else {
                        " "
                    };
                    println!(
                        "{} T{} P{} {}",
                        marker,
                        choice.tab_index + 1,
                        choice.pane_id,
                        choice.title
                    );
                }

                println!();
                println!("Up/Down: select   Enter: choose   f: fullscreen   Esc: close");
            }
            Step::Diff => {
                if let (Some(a), Some(b)) = (&self.pane_a, &self.pane_b) {
                    println!("- T{} P{} {}", a.tab_index + 1, a.pane_id, a.title);
                    println!("+ T{} P{} {}", b.tab_index + 1, b.pane_id, b.title);
                    println!();
                }

                let available = rows.saturating_sub(9).max(1);
                let end = (self.diff_offset + available).min(self.diff.len());
                for line in &self.diff[self.diff_offset..end] {
                    let line: String = line.chars().take(cols).collect();
                    println!("{}", line);
                }

                println!();
                println!(
                    "Up/Down/PgUp/PgDn: scroll   c: copy   f: fullscreen   r: new diff   Esc: close"
                );
            }
        }
    }
}
