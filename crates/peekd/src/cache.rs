use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use peek_core::tools::ToolScripts;

#[derive(Debug)]
struct CacheEntry {
    tools: Vec<ToolScripts>,
    mtimes: HashMap<PathBuf, SystemTime>,
}

#[derive(Debug, Default)]
pub struct DirectoryCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

/// Files we track for cache invalidation per directory.
const TRACKED_FILES: &[&str] = &[
    "package.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "package-lock.json",
    "bun.lockb",
    "bun.lock",
    "Makefile",
    "makefile",
    "GNUmakefile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "Cargo.toml",
];

impl DirectoryCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached tools for a directory, or None if stale/missing.
    pub fn get(&self, dir: &Path) -> Option<&[ToolScripts]> {
        let entry = self.entries.get(dir)?;

        // Check if any tracked file has changed
        for (path, cached_mtime) in &entry.mtimes {
            match path.metadata() {
                Ok(meta) => {
                    if let Ok(mtime) = meta.modified() {
                        if mtime != *cached_mtime {
                            return None;
                        }
                    }
                }
                Err(_) => return None, // File was deleted
            }
        }

        Some(&entry.tools)
    }

    /// Insert or update the cache for a directory.
    pub fn insert(&mut self, dir: PathBuf, tools: Vec<ToolScripts>) {
        let mtimes = collect_mtimes(&dir);
        self.entries.insert(dir, CacheEntry { tools, mtimes });
    }

    /// Invalidate the cache for a directory.
    pub fn invalidate(&mut self, dir: &Path) {
        self.entries.remove(dir);
    }

    /// List all cached directories.
    pub fn cached_dirs(&self) -> Vec<&Path> {
        self.entries.keys().map(|p| p.as_path()).collect()
    }
}

fn collect_mtimes(dir: &Path) -> HashMap<PathBuf, SystemTime> {
    let mut mtimes = HashMap::new();
    for name in TRACKED_FILES {
        let path = dir.join(name);
        if let Ok(meta) = path.metadata() {
            if let Ok(mtime) = meta.modified() {
                mtimes.insert(path, mtime);
            }
        }
    }
    mtimes
}

/// Returns the list of files we should watch in a directory.
pub fn tracked_files_in(dir: &Path) -> Vec<PathBuf> {
    TRACKED_FILES
        .iter()
        .map(|name| dir.join(name))
        .filter(|p| p.exists())
        .collect()
}
