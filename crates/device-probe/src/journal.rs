use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    time::SystemTime,
};

/// Synchronized JSONL evidence; never overwrites a previous hardware capture.
pub struct Journal {
    file: File,
    sequence: u64,
}

/// Recover only newline-terminated, sequential records. An incomplete final
/// write is reported; corruption of a complete record is never hidden.
pub fn recover(bytes: &[u8]) -> Result<(Vec<Value>, bool), String> {
    let complete = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut rows = Vec::new();
    let mut previous_ms = 0;
    for line in bytes[..complete]
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
    {
        let row: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        let ms = row["elapsed_ms"].as_u64().ok_or("Missing elapsed_ms")?;
        if row["sequence"].as_u64() != Some(rows.len() as u64)
            || ms < previous_ms
            || !row["event"].is_string()
        {
            return Err("Invalid journal sequence/time/event".into());
        }
        if rows.is_empty() && (row["schema"] != 1 || row["event"] != "session_check_created") {
            return Err("Unsupported journal schema/header".into());
        }
        previous_ms = ms;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("No committed session header".into());
    }
    Ok((rows, complete != bytes.len()))
}
impl Journal {
    pub fn create(path: &Path) -> Result<Self, String> {
        let mut journal = Self {
            file: OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|e| e.to_string())?,
            sequence: 0,
        };
        let utc_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        journal.record(
            0,
            json!({"event":"session_check_created", "utc_unix_ms":utc_ms, "schema":1}),
        )?;
        Ok(journal)
    }
    pub fn record(&mut self, elapsed_ms: u64, mut row: Value) -> Result<(), String> {
        row["sequence"] = self.sequence.into();
        row["elapsed_ms"] = elapsed_ms.into();
        let mut data = serde_json::to_vec(&row).map_err(|e| e.to_string())?;
        data.push(b'\n');
        self.file
            .write_all(&data)
            .and_then(|_| self.file.sync_all())
            .map_err(|e| e.to_string())?;
        self.sequence += 1;
        // The durable journal is authoritative. A closed UI pipe must not panic
        // before the worker can restore flat grade and acknowledge Pause.
        let _ = std::io::stdout().lock().write_all(&data);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_is_exclusive_and_records_ordered_evidence() {
        let path =
            std::env::temp_dir().join(format!("openride-command-log-{}.jsonl", std::process::id()));
        let mut j = Journal::create(&path).unwrap();
        assert!(Journal::create(&path).is_err());
        j.record(25, json!({"event":"command_requested","opcode":7}))
            .unwrap();
        drop(j);
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows[1]["sequence"], 1);
        assert_eq!(rows[1]["elapsed_ms"], 25);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn recovery_discards_only_a_torn_final_line() {
        let input = b"{\"schema\":1,\"event\":\"session_check_created\",\"sequence\":0,\"elapsed_ms\":0}\n{\"event\":\"command_requested\",\"sequence\":1,\"elapsed_ms\":20}\n{\"event\":";
        let (rows, partial) = recover(input).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(partial);
        let mut corrupt = input.to_vec();
        corrupt.push(b'\n');
        assert!(recover(&corrupt).is_err());
    }
}
