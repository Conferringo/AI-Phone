use crate::Result;
use std::time::Duration;
use tokio::time::Instant;
use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

pub(super) fn is_xterm(class: &str) -> bool {
    class.eq_ignore_ascii_case("xterm") || class.eq_ignore_ascii_case("uxterm")
}

/// A private, short-lived PRIMARY selection. Only the selected XTerm can read it.
pub(super) struct Selection {
    connection: RustConnection,
    window: Window,
    targets: Atom,
    utf8: Atom,
}

impl Selection {
    pub(super) fn open(display: &str) -> Result<Self> {
        let (connection, index) = x11rb::connect(Some(display))?;
        let root = connection.setup().roots[index].root;
        let window = connection.generate_id()?;
        let targets = connection.intern_atom(false, b"TARGETS")?.reply()?.atom;
        let utf8 = connection.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
        connection
            .create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new(),
            )?
            .check()?;
        connection
            .set_selection_owner(window, u32::from(AtomEnum::PRIMARY), x11rb::CURRENT_TIME)?
            .check()?;
        Ok(Self {
            connection,
            window,
            targets,
            utf8,
        })
    }

    fn belongs_to(&self, mut window: Window, terminal: Window) -> bool {
        for _ in 0..12 {
            if window == terminal {
                return true;
            }
            let Some(tree) = self
                .connection
                .query_tree(window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            else {
                break;
            };
            if tree.parent == 0 || tree.parent == tree.root {
                break;
            }
            window = tree.parent;
        }
        false
    }

    pub(super) async fn confirm(self, terminal: Window, text: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            match self.connection.poll_for_event()? {
                Some(Event::SelectionRequest(request)) => {
                    if self.reply(request, terminal, text)? {
                        return Ok(());
                    }
                }
                Some(Event::SelectionClear(_)) => break,
                Some(_) => {}
                None => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
        Err("没有收到终端文本选择请求".into())
    }

    fn reply(&self, request: SelectionRequestEvent, terminal: Window, text: &str) -> Result<bool> {
        let property = if request.property == 0 {
            request.target
        } else {
            request.property
        };
        let allowed = request.owner == self.window
            && request.selection == u32::from(AtomEnum::PRIMARY)
            && self.belongs_to(request.requestor, terminal);
        let is_string = request.target == u32::from(AtomEnum::STRING) && text.is_ascii();
        let delivered = allowed && (request.target == self.utf8 || is_string);
        let accepted = delivered || allowed && request.target == self.targets;
        if delivered {
            self.connection
                .change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    request.target,
                    text.as_bytes(),
                )?
                .check()?;
        } else if accepted {
            let types = [self.targets, self.utf8, AtomEnum::STRING.into()];
            let types = &types[..if text.is_ascii() { 3 } else { 2 }];
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    types,
                )?
                .check()?;
        }
        self.connection
            .send_event(
                false,
                request.requestor,
                EventMask::NO_EVENT,
                SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property: if accepted { property } else { 0 },
                },
            )?
            .check()?;
        Ok(delivered)
    }
}
