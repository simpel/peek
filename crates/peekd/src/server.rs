use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use peek_core::fuzzy;
use peek_core::protocol::{Request, Response, Suggestion};
use peek_core::tools;

use crate::config::Config;
use crate::frecency::FrecencyEngine;
use crate::scanner::Scanner;
use crate::watcher::FileWatcher;

pub struct Server {
    listener: UnixListener,
    scanner: Arc<Scanner>,
    frecency: Arc<Mutex<FrecencyEngine>>,
    config: Arc<Config>,
    start_time: std::time::Instant,
    watcher: Arc<RwLock<FileWatcher>>,
}

impl Server {
    pub fn new(
        listener: UnixListener,
        scanner: Arc<Scanner>,
        frecency: Arc<Mutex<FrecencyEngine>>,
        config: Arc<Config>,
        watcher: Arc<RwLock<FileWatcher>>,
    ) -> Self {
        Self {
            listener,
            scanner,
            frecency,
            config,
            start_time: std::time::Instant::now(),
            watcher,
        }
    }

    pub async fn run(self) -> Result<()> {
        info!("peek daemon listening");

        loop {
            match self.listener.accept().await {
                Ok((stream, _addr)) => {
                    let scanner = self.scanner.clone();
                    let frecency = self.frecency.clone();
                    let config = self.config.clone();
                    let start_time = self.start_time;
                    let watcher = self.watcher.clone();

                    tokio::spawn(async move {
                        let (reader, mut writer) = stream.into_split();
                        let mut reader = BufReader::new(reader);
                        let mut line = String::new();

                        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                            let response = match serde_json::from_str::<Request>(line.trim()) {
                                Ok(request) => {
                                    handle_request(
                                        request, &scanner, &frecency, &config, start_time,
                                        &watcher,
                                    )
                                    .await
                                }
                                Err(e) => Response::Error {
                                    message: format!("invalid request: {e}"),
                                },
                            };

                            let mut response_json = serde_json::to_string(&response)
                                .unwrap_or_else(|_| {
                                    r#"{"type":"error","message":"serialization failed"}"#
                                        .to_string()
                                });
                            response_json.push('\n');

                            if let Err(e) = writer.write_all(response_json.as_bytes()).await {
                                debug!("client disconnected: {e}");
                                break;
                            }

                            line.clear();
                        }
                    });
                }
                Err(e) => {
                    error!("accept error: {e}");
                }
            }
        }
    }
}

async fn handle_request(
    request: Request,
    scanner: &Scanner,
    frecency: &Mutex<FrecencyEngine>,
    config: &Config,
    start_time: std::time::Instant,
    watcher: &RwLock<FileWatcher>,
) -> Response {
    match request {
        Request::Suggest { cwd, line, .. } => {
            handle_suggest(&cwd, &line, scanner, frecency, config).await
        }
        Request::Cd { cwd } => {
            let dir = PathBuf::from(&cwd);
            let _ = scanner.get_tools(&dir).await;
            let mut w = watcher.write().await;
            let _ = w.watch_directory(&dir).await;
            Response::Ack
        }
        Request::Executed {
            cwd,
            command,
            tool,
        } => {
            if let Ok(engine) = frecency.lock() {
                if let Err(e) = engine.record(&cwd, &command, &tool) {
                    error!("failed to record execution: {e}");
                }
            }
            Response::Ack
        }
        Request::Status => {
            let w = watcher.read().await;
            let dirs = w.watched_dirs().await;
            Response::Status {
                pid: std::process::id(),
                watched_dirs: dirs.iter().map(|d| d.to_string_lossy().to_string()).collect(),
                uptime_secs: start_time.elapsed().as_secs(),
            }
        }
        Request::Shutdown => {
            info!("shutdown requested");
            std::process::exit(0);
        }
    }
}

async fn handle_suggest(
    cwd: &str,
    line: &str,
    scanner: &Scanner,
    frecency: &Mutex<FrecencyEngine>,
    config: &Config,
) -> Response {
    let dir = PathBuf::from(cwd);

    let (tool, filter) = match tools::match_tool_prefix(line) {
        Some((t, f)) => (t, f),
        None => {
            return Response::Suggestions {
                suggestions: vec![],
                tool: String::new(),
            }
        }
    };

    if !config.is_tool_enabled(tool) {
        return Response::Suggestions {
            suggestions: vec![],
            tool: tool.name().to_string(),
        };
    }

    let all_tools = scanner.get_tools(&dir).await;
    let tool_scripts = match all_tools.iter().find(|ts| ts.tool == tool) {
        Some(ts) => ts,
        None => {
            return Response::Suggestions {
                suggestions: vec![],
                tool: tool.name().to_string(),
            }
        }
    };

    let candidates: Vec<&str> = tool_scripts.entries.iter().map(|e| e.name.as_str()).collect();
    let matches = fuzzy::fuzzy_match(filter, &candidates);

    let frecency_scores: HashMap<String, f64> = frecency
        .lock()
        .ok()
        .and_then(|engine| engine.scores(cwd).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(cmd, _, score)| (cmd, score))
        .collect();

    let mut suggestions: Vec<Suggestion> = matches
        .iter()
        .take(config.max_suggestions)
        .map(|m| {
            let entry = &tool_scripts.entries[m.index];
            let frecency_score = frecency_scores.get(&entry.name).copied().unwrap_or(0.0);
            let combined_score = m.score as f64 + frecency_score;
            Suggestion {
                name: entry.name.clone(),
                preview: entry.preview.clone(),
                score: combined_score,
            }
        })
        .collect();

    suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Response::Suggestions {
        suggestions,
        tool: tool.name().to_string(),
    }
}
