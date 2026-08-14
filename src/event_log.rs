use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

use crate::events::Event;

#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct JsonlLog<E> {
    path: PathBuf,
    file: Mutex<Option<tokio::fs::File>>,
    _marker: std::marker::PhantomData<E>,
}

pub type EventLog = JsonlLog<Event>;

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

    pub async fn ensure_parent_dir(&self) -> Result<(), EventLogError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    pub async fn append(&self, event: &E) -> Result<(), EventLogError> {
        self.ensure_parent_dir().await?;
        let mut line = serde_json::to_string(event)?;
        line.push('\n');
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
        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }

    pub async fn load_all(&self) -> Result<(Vec<E>, Vec<(usize, String)>), EventLogError> {
        if !fs::try_exists(&self.path).await? {
            return Ok((vec![], vec![]));
        }
        let f = fs::File::open(&self.path).await?;
        let mut reader = BufReader::new(f).lines();
        let mut events = Vec::new();
        let mut bad = Vec::new();
        let mut line_no = 0usize;
        while let Some(line) = reader.next_line().await? {
            line_no += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<E>(trimmed) {
                Ok(ev) => events.push(ev),
                Err(_) => bad.push((line_no, line)),
            }
        }
        Ok((events, bad))
    }
}
