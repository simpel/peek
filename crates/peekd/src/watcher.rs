use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, warn};

use crate::cache::{tracked_files_in, DirectoryCache};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    watched_dirs: Arc<RwLock<HashSet<PathBuf>>>,
}

impl FileWatcher {
    pub fn new(
        cache: Arc<RwLock<DirectoryCache>>,
    ) -> Result<(Self, mpsc::Receiver<PathBuf>)> {
        let (invalidate_tx, invalidate_rx) = mpsc::channel::<PathBuf>(64);
        let watched_dirs = Arc::new(RwLock::new(HashSet::new()));

        let cache_clone = cache.clone();
        let tx = invalidate_tx.clone();

        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    for path in &event.paths {
                        if let Some(parent) = path.parent() {
                            let parent = parent.to_path_buf();
                            let cache_clone = cache_clone.clone();
                            let tx = tx.clone();
                            // Spawn a blocking task to invalidate cache
                            std::thread::spawn(move || {
                                let rt = tokio::runtime::Handle::try_current();
                                if let Ok(handle) = rt {
                                    handle.block_on(async {
                                        let mut cache = cache_clone.write().await;
                                        cache.invalidate(&parent);
                                        let _ = tx.send(parent).await;
                                    });
                                }
                            });
                        }
                    }
                }
                Err(e) => {
                    error!("file watcher error: {e}");
                }
            }
        })?;

        Ok((
            Self {
                watcher,
                watched_dirs,
            },
            invalidate_rx,
        ))
    }

    /// Start watching tracked files in a directory.
    pub async fn watch_directory(&mut self, dir: &PathBuf) -> Result<()> {
        {
            let dirs = self.watched_dirs.read().await;
            if dirs.contains(dir) {
                return Ok(());
            }
        }

        let files = tracked_files_in(dir);
        for file in &files {
            debug!("watching {}", file.display());
            if let Err(e) = self.watcher.watch(file, RecursiveMode::NonRecursive) {
                warn!("failed to watch {}: {e}", file.display());
            }
        }

        let mut dirs = self.watched_dirs.write().await;
        dirs.insert(dir.clone());

        Ok(())
    }

    pub async fn watched_dirs(&self) -> Vec<PathBuf> {
        let dirs = self.watched_dirs.read().await;
        dirs.iter().cloned().collect()
    }
}
