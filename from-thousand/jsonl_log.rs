//! Generic append-only JSONL log (one file handle, batched `fsync`).

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

#[derive(Debug, thiserror::Error)]
pub enum JsonlLogError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Append-only JSONL backed by **one** `File` handle (lazy-opened on first write).
///
/// Use [`append_batch`](Self::append_batch) for group commit: many lines, one `fsync`.
pub struct JsonlLog<E> {
    path: PathBuf,
    file: Mutex<Option<tokio::fs::File>>,
    _marker: std::marker::PhantomData<E>,
}

impl<E> JsonlLog<E>
where
    E: Serialize + DeserializeOwned,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            file: Mutex::new(None),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn ensure_parent_dir(&self) -> Result<(), JsonlLogError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    /// Append any number of lines, flush to the OS, then **`fsync`** once.
    pub async fn append_batch(&self, events: &[E]) -> Result<(), JsonlLogError> {
        if events.is_empty() {
            return Ok(());
        }
        self.ensure_parent_dir().await?;
        let mut buf = String::new();
        for event in events {
            let mut line = serde_json::to_string(event)?;
            line.push('\n');
            buf.push_str(&line);
        }

        let mut guard = self.file.lock().await;
        if guard.is_none() {
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .await?;
            *guard = Some(f);
        }
        let file = guard.as_mut().expect("just set");
        file.write_all(buf.as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }

    pub async fn append(&self, event: &E) -> Result<(), JsonlLogError> {
        self.append_batch(std::slice::from_ref(event)).await
    }

    /// Load all lines. Corrupt lines are skipped and returned as `(line_no, line)`.
    pub async fn load_all(&self) -> Result<(Vec<E>, Vec<(usize, String)>), JsonlLogError> {
        if !fs::try_exists(&self.path).await? {
            return Ok((vec![], vec![]));
        }

        let f = fs::File::open(&self.path).await?;
        let mut reader = BufReader::new(f).lines();

        let mut events = Vec::new();
        let mut bad_lines = Vec::new();

        let mut line_no: usize = 0;
        while let Some(line) = reader.next_line().await? {
            line_no += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<E>(trimmed) {
                Ok(ev) => events.push(ev),
                Err(_) => bad_lines.push((line_no, line)),
            }
        }

        Ok((events, bad_lines))
    }
}
