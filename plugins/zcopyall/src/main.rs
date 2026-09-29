use std::collections::BTreeMap;
use zellij_tile::prelude::*;

register_plugin!(State);

#[derive(Default)]
struct State {
    permissions_granted: bool,
    copy_pending: bool,
}

impl State {
    fn request_copy(&mut self) {
        self.copy_pending = true;

        if self.permissions_granted {
            list_clients();
        }
    }

    fn copy_for_current_client(&mut self, clients: Vec<ClientInfo>) {
        if !self.copy_pending {
            return;
        }

        self.copy_pending = false;

        let Some(client) = clients.into_iter().find(|client| client.is_current_client) else {
            eprintln!("zcopyall: could not identify current Zellij client");
            return;
        };

        let pane_id = client.pane_id;

        let contents = match get_pane_scrollback(pane_id, true) {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("zcopyall: failed reading {:?}: {}", pane_id, error);
                return;
            }
        };

        let mut lines = contents.lines_above_viewport;
        lines.extend(contents.viewport);
        lines.extend(contents.lines_below_viewport);

        let text = lines.join("\n");

        if text.is_empty() {
            eprintln!("zcopyall: pane scrollback was empty");
            return;
        }

        copy_to_clipboard(text);
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::PermissionRequestResult,
            EventType::ListClients,
            EventType::SystemClipboardFailure,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadPaneContents,
            PermissionType::WriteToClipboard,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.permissions_granted = true;

                if self.copy_pending {
                    list_clients();
                }

                hide_self();
            }

            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.permissions_granted = false;
                eprintln!("zcopyall: required permissions were denied");
            }

            Event::ListClients(clients) => {
                self.copy_for_current_client(clients);
            }

            Event::SystemClipboardFailure => {
                eprintln!("zcopyall: Zellij reported a system clipboard failure");
            }

            _ => {}
        }

        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "copy_all" {
            self.request_copy();
        }

        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!("zcopyall");
        println!();
        println!("Grant the requested permissions.");
        println!("Afterward this plugin remains hidden in the background.");
    }
}
