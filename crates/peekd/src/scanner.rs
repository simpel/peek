use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::cache::DirectoryCache;
use crate::config::Config;
use peek_core::tools::{self, ToolScripts};

pub struct Scanner {
    cache: Arc<RwLock<DirectoryCache>>,
    config: Arc<Config>,
}

impl Scanner {
    pub fn new(cache: Arc<RwLock<DirectoryCache>>, config: Arc<Config>) -> Self {
        Self { cache, config }
    }

    /// Get tools for a directory, using cache if available.
    pub async fn get_tools(&self, dir: &Path) -> Vec<ToolScripts> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(tools) = cache.get(dir) {
                return tools.to_vec();
            }
        }

        // Cache miss — scan
        let mut tools = tools::scan_directory(dir);

        // Filter disabled tools
        tools.retain(|ts| self.config.is_tool_enabled(ts.tool));

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(dir.to_path_buf(), tools.clone());
        }

        tools
    }

    /// Force a rescan of a directory.
    pub async fn invalidate(&self, dir: &Path) {
        let mut cache = self.cache.write().await;
        cache.invalidate(dir);
    }
}
