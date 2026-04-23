use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Suggest {
        cwd: String,
        line: String,
        cursor: usize,
    },
    Cd {
        cwd: String,
    },
    Executed {
        cwd: String,
        command: String,
        tool: String,
    },
    Status,
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Suggestion {
    pub name: String,
    pub preview: String,
    pub score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Suggestions {
        suggestions: Vec<Suggestion>,
        tool: String,
    },
    Ack,
    Status {
        pid: u32,
        watched_dirs: Vec<String>,
        uptime_secs: u64,
    },
    Error {
        message: String,
    },
}
