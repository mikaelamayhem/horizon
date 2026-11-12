//! Looking for every DualSense plugged in or paired, on a thread of its own: listing the system's
//! HID devices can take tens of milliseconds on Windows, too long for a frame. Splitscreen opens
//! a [`Link`](crate::Link) for each pad it finds ([`Link::start_path`](crate::Link::start_path)).
use crate::is_dualsense;
use hidapi::HidApi;
use std::ffi::CString;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the list is made again: nothing tells the game a pad was plugged in.
const EVERY: Duration = Duration::from_secs(3);

/// The DualSenses' HID paths, each once, as hidapi lists them now.
fn paths(api: &HidApi) -> Vec<CString> {
    let mut paths: Vec<CString> = api
        .device_list()
        .filter(|d| is_dualsense(d.vendor_id(), d.product_id()))
        .map(|d| d.path().to_owned())
        .collect();
    paths.dedup();
    paths
}

pub struct Finder {
    found: Receiver<Vec<CString>>,
    /// Closing it ends the thread.
    stop: Option<Sender<()>>,
    latest: Vec<CString>,
    thread: Option<JoinHandle<()>>,
}

impl Finder {
    pub fn start() -> Finder {
        let (send, found) = mpsc::channel();
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("dualsense finder".into())
            .spawn(move || {
                let mut api = match HidApi::new() {
                    Ok(api) => api,
                    Err(e) => {
                        log::warn!("Horizon: can't look for DualSenses: {e}");
                        return;
                    }
                };
                loop {
                    if send.send(paths(&api)).is_err() {
                        return;
                    }
                    match stopped.recv_timeout(EVERY) {
                        Err(RecvTimeoutError::Timeout) => {}
                        _ => return,
                    }
                    if let Err(e) = api.refresh_devices() {
                        log::warn!("Horizon: can't look for DualSenses: {e}");
                    }
                }
            })
            .ok();
        Finder {
            found,
            stop: Some(stop),
            latest: Vec::new(),
            thread,
        }
    }

    /// The pads found at the last look.
    pub fn paths(&mut self) -> &[CString] {
        while let Ok(paths) = self.found.try_recv() {
            self.latest = paths;
        }
        &self.latest
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        self.stop = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
