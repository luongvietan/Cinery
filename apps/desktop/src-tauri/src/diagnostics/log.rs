use crate::diagnostics::redaction::DiagnosticsRedactor;
use crate::error::AppError;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOG_FILE_NAME: &str = "logs.txt";

/// Append-only structured application log stored under the project's
/// `diagnostics/` folder. Every line is redacted before it touches disk so
/// credentials can never reach the filesystem through a log call.
pub struct DiagnosticLog {
    directory: PathBuf,
    lock: Mutex<()>,
}

impl DiagnosticLog {
    pub fn new(project_root: &Path) -> Self {
        Self {
            directory: project_root.join("diagnostics"),
            lock: Mutex::new(()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn log_file(&self) -> PathBuf {
        self.directory.join(LOG_FILE_NAME)
    }

    /// Append one structured event. Creates the diagnostics folder on first
    /// use. Fails only when the filesystem refuses the write.
    pub fn append(
        &self,
        subsystem: &str,
        event: &str,
        correlation_id: Option<&str>,
        message: &str,
    ) -> Result<(), AppError> {
        let line = format_line(
            chrono::Utc::now().to_rfc3339(),
            subsystem,
            event,
            correlation_id,
            message,
        );
        self.write_line(&line)
    }

    /// Read the current log contents, oldest line first.
    pub fn read_all(&self) -> Result<String, AppError> {
        let path = self.log_file();
        if !path.exists() {
            return Ok(String::new());
        }
        fs::read_to_string(&path).map_err(|e| AppError::FileSystem(e.to_string()))
    }

    fn write_line(&self, line: &str) -> Result<(), AppError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::FileSystem("diagnostics log lock poisoned".into()))?;
        fs::create_dir_all(&self.directory)
            .map_err(|e| AppError::FileSystem(e.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_file())
            .map_err(|e| AppError::FileSystem(e.to_string()))?;
        file.write_all(line.as_bytes())
            .map_err(|e| AppError::FileSystem(e.to_string()))?;
        Ok(())
    }
}

fn format_line(
    timestamp: String,
    subsystem: &str,
    event: &str,
    correlation_id: Option<&str>,
    message: &str,
) -> String {
    let redacted = DiagnosticsRedactor::redact_string(message);
    let correlation = correlation_id.unwrap_or("-");
    format!(
        "{timestamp}\t{subsystem}\t{event}\t{correlation}\t{redacted}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_tab_separated_redacted_lines() {
        let temp = tempdir().unwrap();
        let log = DiagnosticLog::new(temp.path());

        log.append(
            "workflow",
            "run_failed",
            Some("run-123"),
            "Authorization: Bearer sk-test-secret-value",
        )
        .unwrap();

        let contents = log.read_all().unwrap();
        assert!(contents.contains("workflow\trun_failed\trun-123\t"));
        assert!(!contents.contains("sk-test-secret-value"));
        assert!(contents.contains("[REDACTED]"));
    }

    #[test]
    fn multiple_appends_are_ordered() {
        let temp = tempdir().unwrap();
        let log = DiagnosticLog::new(temp.path());

        log.append("a", "one", None, "first").unwrap();
        log.append("b", "two", None, "second").unwrap();

        let contents = log.read_all().unwrap();
        let first = contents.find("first").unwrap();
        let second = contents.find("second").unwrap();
        assert!(first < second);
    }

    #[test]
    fn read_all_returns_empty_for_missing_log() {
        let temp = tempdir().unwrap();
        let log = DiagnosticLog::new(temp.path());

        assert_eq!(log.read_all().unwrap(), "");
    }

    #[test]
    fn log_directory_is_diagnostics_subfolder() {
        let temp = tempdir().unwrap();
        let log = DiagnosticLog::new(temp.path());

        assert_eq!(
            log.directory(),
            temp.path().join("diagnostics").as_path()
        );
        assert_eq!(
            log.log_file(),
            temp.path().join("diagnostics").join("logs.txt")
        );
    }
}
