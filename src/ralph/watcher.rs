use anyhow::Result;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, FileIdMap, new_debouncer};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

/// Carries the path of a changed `.json` file.
pub struct WatcherEvent {
    pub path: PathBuf,
}

/// Holds the debouncer handle. Dropping this value stops OS-native file watching.
pub struct Watcher {
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
}

impl Watcher {
    /// Start watching `root` recursively. For every debounced Create, Modify, or
    /// Remove/Rename event whose path has a `.json` extension, a `WatcherEvent` is
    /// sent on `tx`. Access-only and other non-data events are silently dropped.
    ///
    /// The returned `Watcher` keeps the OS watcher alive; dropping it stops watching.
    pub fn start(root: &Path, tx: Sender<WatcherEvent>) -> Result<Watcher> {
        let mut debouncer: Debouncer<RecommendedWatcher, FileIdMap> = new_debouncer(
            Duration::from_millis(50),
            None,
            move |result: DebounceEventResult| {
                let events = match result {
                    Ok(events) => events,
                    Err(_) => return,
                };
                for debounced in events {
                    use notify::EventKind;
                    match debounced.event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
                        _ => continue,
                    }
                    for path in &debounced.event.paths {
                        if path.extension().and_then(|e| e.to_str()) == Some("json") {
                            let _ = tx.try_send(WatcherEvent { path: path.clone() });
                        }
                    }
                }
            },
        )?;

        // Bring notify::Watcher trait into scope without shadowing our Watcher struct.
        use notify::Watcher as _;
        debouncer.watcher().watch(root, RecursiveMode::Recursive)?;

        Ok(Watcher { _debouncer: debouncer })
    }
}
