use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::FrecencyConfig;

pub struct FrecencyEngine {
    conn: Connection,
    config: FrecencyConfig,
}

impl FrecencyEngine {
    pub fn new(db_path: &Path, config: FrecencyConfig) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS usage (
                directory TEXT NOT NULL,
                command TEXT NOT NULL,
                tool TEXT NOT NULL,
                last_used INTEGER NOT NULL,
                count INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (directory, command, tool)
            )",
        )?;
        Ok(Self { conn, config })
    }

    /// Record that a command was executed.
    pub fn record(&self, directory: &str, command: &str, tool: &str) -> Result<()> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO usage (directory, command, tool, last_used, count)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(directory, command, tool) DO UPDATE SET
                last_used = ?4,
                count = count + 1",
            rusqlite::params![directory, command, tool, now],
        )?;
        Ok(())
    }

    /// Get frecency scores for all commands in a directory.
    pub fn scores(&self, directory: &str) -> Result<Vec<(String, String, f64)>> {
        let now = now_secs();
        let half_life_secs = self.config.recency_half_life_days * 86400.0;
        let ln2 = std::f64::consts::LN_2;

        let mut stmt = self.conn.prepare(
            "SELECT command, tool, last_used, count FROM usage WHERE directory = ?1",
        )?;

        let results = stmt
            .query_map(rusqlite::params![directory], |row| {
                let command: String = row.get(0)?;
                let tool: String = row.get(1)?;
                let last_used: i64 = row.get(2)?;
                let count: i64 = row.get(3)?;

                let age_secs = (now - last_used) as f64;
                let recency = (-ln2 * age_secs / half_life_secs).exp();
                let score = self.config.frequency_weight * count as f64
                    + self.config.recency_weight * recency;

                Ok((command, tool, score))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Clear all history.
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM usage", [])?;
        Ok(())
    }

    /// Clear history for a specific directory.
    pub fn clear_directory(&self, directory: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM usage WHERE directory = ?1",
            rusqlite::params![directory],
        )?;
        Ok(())
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, FrecencyEngine) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let engine = FrecencyEngine::new(&db_path, FrecencyConfig::default()).unwrap();
        (dir, engine)
    }

    #[test]
    fn test_record_and_score() {
        let (_dir, engine) = temp_db();
        engine.record("/project", "dev", "pnpm").unwrap();
        engine.record("/project", "dev", "pnpm").unwrap();
        engine.record("/project", "build", "pnpm").unwrap();

        let scores = engine.scores("/project").unwrap();
        assert_eq!(scores.len(), 2);

        let dev_score = scores.iter().find(|(cmd, _, _)| cmd == "dev").unwrap().2;
        let build_score = scores.iter().find(|(cmd, _, _)| cmd == "build").unwrap().2;
        // dev was run twice, so it should have a higher score
        assert!(dev_score > build_score);
    }

    #[test]
    fn test_clear() {
        let (_dir, engine) = temp_db();
        engine.record("/project", "dev", "pnpm").unwrap();
        engine.clear().unwrap();
        let scores = engine.scores("/project").unwrap();
        assert!(scores.is_empty());
    }
}
