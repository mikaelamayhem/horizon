//! Splitscreen's DualSenses, read directly (`unlisted.rs`) with one link each, so that every
//! player's pad brings its own gyro, touchpad and lights. Each player borrows its pad's link for
//! its own turn (`PadRouter::lend_pad`), as P1 alone borrows the router's reader.
use super::{PadId, unlisted};
use crate::game::pad::{PadEvent, Pads};
use horizon_client::gamepad::{Button, PadState};
use horizon_dualsense::{Link, finder::Finder};
use std::ffi::CString;

struct DirectPad {
    key: u32,
    path: CString,
    /// `None` while a player borrows it.
    link: Option<Link>,
    watch: unlisted::Watch,
    /// Its controls at the last read, for its presses; `None` while it is gone.
    state: Option<PadState>,
}

#[derive(Default)]
pub(super) struct Direct {
    finder: Option<Finder>,
    pads: Vec<DirectPad>,
    next_key: u32,
}

impl Direct {
    /// Open a link for each DualSense found, and read them all: presses, and pads going and
    /// coming back.
    pub(super) fn read(&mut self, events: &mut Vec<PadEvent>) {
        if !unlisted::wanted() {
            *self = Direct::default();
            return;
        }
        let paths = self
            .finder
            .get_or_insert_with(Finder::start)
            .paths()
            .to_vec();
        for path in &paths {
            if !self.pads.iter().any(|p| &p.path == path) {
                self.next_key += 1;
                self.pads.push(DirectPad {
                    key: self.next_key,
                    path: path.clone(),
                    link: Some(Link::start_path(path.clone())),
                    watch: unlisted::Watch::default(),
                    state: None,
                });
            }
        }
        for p in &mut self.pads {
            let Some(link) = p.link.as_ref() else {
                continue;
            };
            let id = PadId::Unlisted(p.key);
            let (state, change) = p.watch.poll(link);
            match change {
                Some(true) => events.push(PadEvent::Back(id)),
                Some(false) => events.push(PadEvent::Gone(id)),
                None => {}
            }
            if let Some(now) = state {
                let before = p.state.unwrap_or_default();
                events.extend(
                    Button::ALL
                        .into_iter()
                        .filter(|&b| now.held(b) && !before.held(b))
                        .map(|b| PadEvent::Press(id, b)),
                );
            }
            p.state = state;
        }
        // A pad gone for good (no longer listed, and counted as gone): its link closes.
        self.pads
            .retain(|p| p.link.is_none() || paths.contains(&p.path) || p.watch.present());
    }

    fn pad(&self, key: u32) -> Option<&DirectPad> {
        self.pads.iter().find(|p| p.key == key)
    }

    pub(super) fn connected(&self, key: u32) -> bool {
        self.pad(key)
            .is_some_and(|p| p.link.is_none() || p.watch.present())
    }

    pub(super) fn state(&self, key: u32) -> PadState {
        self.pad(key).and_then(|p| p.state).unwrap_or_default()
    }

    /// Lend pad `key`'s link to the player it drives, for that player's turn.
    pub(super) fn lend(&mut self, pads: &mut Pads, key: u32) {
        let Some(p) = self.pads.iter_mut().find(|p| p.key == key) else {
            return;
        };
        let Some(link) = p.link.take() else {
            return;
        };
        pads.borrow_direct(key, link);
    }

    /// The turn is over: the lent link comes back.
    pub(super) fn reclaim(&mut self, pads: &mut Pads) {
        let Some(key) = pads.devices.unlisted.key else {
            return;
        };
        if let Some(p) = self
            .pads
            .iter_mut()
            .find(|p| p.key == key && p.link.is_none())
            && let Some(link) = pads.devices.output.take()
        {
            p.link = Some(link);
        }
    }
}

impl Pads {
    /// Play with a DualSense the router reads directly: its link is this player's output for
    /// the turn (gyro, touchpad, lights, rumble).
    fn borrow_direct(&mut self, key: u32, link: Link) {
        let d = &mut self.devices;
        if d.unlisted.key != Some(key) {
            d.rumbles.clear();
            d.reading_motion = false;
            d.shown = None;
        }
        d.output = Some(link);
        d.unlisted.active = true;
        d.unlisted.key = Some(key);
        d.dualsense = true;
        d.active = None;
    }
}
