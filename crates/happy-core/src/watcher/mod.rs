use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Modified(String),
    Created(String),
    Removed(String),
}

/// Watch a directory for file changes.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<WatchEvent>,
}

impl FileWatcher {
    /// Start watching a directory for changes.
    pub fn new(path: &str) -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    for path in event.paths {
                        let path_str = path.to_string_lossy().to_string();
                        let watch_event = match event.kind {
                            notify::EventKind::Create(_) => WatchEvent::Created(path_str),
                            notify::EventKind::Modify(_) => WatchEvent::Modified(path_str),
                            notify::EventKind::Remove(_) => WatchEvent::Removed(path_str),
                            _ => continue,
                        };
                        let _ = tx.send(watch_event);
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(Path::new(path), RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    /// Try to receive the next event (non-blocking).
    pub fn try_recv(&self) -> Option<WatchEvent> {
        self.receiver.try_recv().ok()
    }

    /// Receive the next event (blocking with timeout).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<WatchEvent> {
        self.receiver.recv_timeout(timeout).ok()
    }
}
