use mcl_spec::value::MclValue;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub path: String,
    pub old_value: MclValue,
    pub new_value: MclValue,
    pub source: Option<String>,
}

impl AuditEntry {
    fn new(path: &str, old_value: MclValue, new_value: MclValue) -> Self {
        Self {
            timestamp: iso_now(),
            path: path.to_string(),
            old_value,
            new_value,
            source: None,
        }
    }
}

fn iso_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO);
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();

    let year = 1970 + (secs / 31_536_000);
    let secs = secs % 31_536_000;
    let days_from_jan = secs / 86_400;
    let time_of_day = secs % 86_400;

    let leap_years = (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    let day_of_year = days_from_jan - leap_years;

    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0;
    let mut day = day_of_year;
    for (i, &md) in month_days.iter().enumerate() {
        if day < md {
            month = i + 1;
            break;
        }
        day -= md;
    }
    if month == 0 {
        month = 12;
    }

    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}

#[derive(Debug, Clone)]
pub struct AuditLog {
    path: Option<std::path::PathBuf>,
}

impl AuditLog {
    pub fn open(path: &Path) -> Result<Self, MclError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            path: Some(path.to_path_buf()),
        })
    }

    pub fn disabled() -> Self {
        Self { path: None }
    }

    pub fn record(
        &mut self,
        path: &str,
        old_value: MclValue,
        new_value: MclValue,
    ) -> Result<(), MclError> {
        let entry = AuditEntry::new(path, old_value, new_value);
        self.write_entry(&entry)
    }

    fn write_entry(&mut self, entry: &AuditEntry) -> Result<(), MclError> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        let mut line = serde_json::to_string(entry)
            .map_err(|e| MclError::State(format!("failed to serialize audit entry: {e}")))?;
        line.push('\n');

        file.write_all(line.as_bytes())?;
        file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn audit_log_records_mutations() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = AuditLog::open(&path).expect("open audit log");

        log.record("player.hp", MclValue::Null, MclValue::Int(100))
            .expect("record");

        log.record("player.hp", MclValue::Int(100), MclValue::Int(50))
            .expect("record");

        let contents = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let entry1: AuditEntry = serde_json::from_str(lines[0]).expect("parse entry 1");
        assert_eq!(entry1.path, "player.hp");
        assert_eq!(entry1.old_value, MclValue::Null);
        assert_eq!(entry1.new_value, MclValue::Int(100));

        let entry2: AuditEntry = serde_json::from_str(lines[1]).expect("parse entry 2");
        assert_eq!(entry2.path, "player.hp");
        assert_eq!(entry2.old_value, MclValue::Int(100));
        assert_eq!(entry2.new_value, MclValue::Int(50));
    }

    #[test]
    fn audit_log_entries_have_timestamps() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = AuditLog::open(&path).expect("open");

        log.record("test.key", MclValue::Null, MclValue::Bool(true))
            .expect("record");

        let contents = std::fs::read_to_string(&path).expect("read");
        let entry: AuditEntry = serde_json::from_str(contents.trim()).expect("parse");

        assert!(!entry.timestamp.is_empty());
        assert!(entry.timestamp.contains('T'));
        assert!(entry.timestamp.ends_with('Z'));
        assert!(entry.timestamp.len() >= 20);
    }

    #[test]
    fn audit_log_is_append_only() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        {
            let mut log = AuditLog::open(&path).expect("open");
            log.record("a", MclValue::Null, MclValue::Int(1))
                .expect("record");
        }

        {
            let mut log = AuditLog::open(&path).expect("reopen");
            log.record("b", MclValue::Null, MclValue::Int(2))
                .expect("record");
        }

        let contents = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let e1: AuditEntry = serde_json::from_str(lines[0]).expect("parse 1");
        assert_eq!(e1.path, "a");

        let e2: AuditEntry = serde_json::from_str(lines[1]).expect("parse 2");
        assert_eq!(e2.path, "b");
    }

    #[test]
    fn audit_log_format_is_valid_jsonl() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = AuditLog::open(&path).expect("open");

        log.record("x", MclValue::Int(1), MclValue::Int(2))
            .expect("record");
        log.record("y", MclValue::Bool(false), MclValue::Bool(true))
            .expect("record");

        let contents = std::fs::read_to_string(&path).expect("read");

        for line in contents.lines() {
            let val: serde_json::Value =
                serde_json::from_str(line).expect("each line should be valid JSON");
            assert!(val.is_object(), "each line should be a JSON object");
            assert!(val.get("timestamp").is_some());
            assert!(val.get("path").is_some());
            assert!(val.get("old_value").is_some());
            assert!(val.get("new_value").is_some());
        }
    }

    #[test]
    fn disabled_audit_log_does_not_write() {
        let mut log = AuditLog::disabled();

        log.record("test", MclValue::Null, MclValue::Int(1))
            .expect("disabled log should succeed silently");
    }

    #[test]
    fn audit_log_handles_all_value_types() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = AuditLog::open(&path).expect("open");

        log.record("v_int", MclValue::Null, MclValue::Int(-42))
            .expect("int");
        log.record("v_float", MclValue::Null, MclValue::Float(3.14))
            .expect("float");
        log.record("v_str", MclValue::Null, MclValue::String("hello".into()))
            .expect("str");
        log.record("v_bool", MclValue::Null, MclValue::Bool(true))
            .expect("bool");
        log.record("v_null", MclValue::Int(1), MclValue::Null)
            .expect("null");

        let contents = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 5);

        for line in &lines {
            let _: AuditEntry = serde_json::from_str(line).expect("parse");
        }
    }
}
