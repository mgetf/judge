//! Exhibit bytes. R2 when the usual Cloudflare/S3 env vars are set; otherwise
//! the judge's data dir, served at `/blobs/…`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusty_s3::actions::{PutObject, S3Action};
use rusty_s3::{Bucket, Credentials, UrlStyle};

pub const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct BlobStore {
    inner: Arc<Inner>,
}

enum Inner {
    Local {
        root: PathBuf,
        public_base: String,
    },
    R2 {
        bucket: Bucket,
        creds: Credentials,
        public_base: String,
        http: reqwest::Client,
    },
}

impl BlobStore {
    pub fn from_env(data_dir: &Path, public_url: &str) -> Self {
        match R2Settings::from_env() {
            Ok(Some(r2)) => match Self::r2(r2) {
                Ok(store) => {
                    tracing::info!("exhibits go to R2");
                    store
                }
                Err(e) => {
                    tracing::warn!("R2 config unusable ({e}); exhibits stay on disk");
                    Self::local(data_dir.join("blobs"), public_url)
                }
            },
            Ok(None) => Self::local(data_dir.join("blobs"), public_url),
            Err(e) => {
                tracing::warn!("R2 env incomplete ({e}); exhibits stay on disk");
                Self::local(data_dir.join("blobs"), public_url)
            }
        }
    }

    pub fn local(root: PathBuf, public_url: &str) -> Self {
        Self {
            inner: Arc::new(Inner::Local {
                root,
                public_base: format!("{}/blobs", public_url.trim_end_matches('/')),
            }),
        }
    }

    fn r2(cfg: R2Settings) -> Result<Self, String> {
        let endpoint = cfg
            .endpoint
            .parse()
            .map_err(|e| format!("R2 endpoint: {e}"))?;
        let bucket = Bucket::new(endpoint, UrlStyle::Path, cfg.bucket, "auto")
            .map_err(|e| format!("R2 bucket: {e}"))?;
        Ok(Self {
            inner: Arc::new(Inner::R2 {
                bucket,
                creds: Credentials::new(cfg.access_key, cfg.secret_key),
                public_base: cfg.public_base.trim_end_matches('/').to_string(),
                http: reqwest::Client::new(),
            }),
        })
    }

    pub fn is_local(&self) -> bool {
        matches!(&*self.inner, Inner::Local { .. })
    }

    /// Store `bytes` under `key`. Returns the public href.
    pub async fn put(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> Result<String, String> {
        if bytes.len() > MAX_BYTES {
            return Err(format!("file larger than {MAX_BYTES} bytes"));
        }
        let key = sanitize_key(key)?;
        match &*self.inner {
            Inner::Local { root, public_base } => {
                let path = root.join(&key);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                let _ = content_type;
                Ok(format!("{public_base}/{key}"))
            }
            Inner::R2 {
                bucket,
                creds,
                public_base,
                http,
            } => {
                let mut action = PutObject::new(bucket, Some(creds), &key);
                action
                    .headers_mut()
                    .insert("content-type", content_type.to_string());
                let url = action.sign(Duration::from_secs(600));
                let resp = http
                    .put(url)
                    .header("content-type", content_type)
                    .body(bytes.to_vec())
                    .send()
                    .await
                    .map_err(|e| format!("R2 put: {e}"))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("R2 put {status}: {body}"));
                }
                Ok(format!("{public_base}/{key}"))
            }
        }
    }

    pub fn get_local(&self, key: &str) -> Option<(Vec<u8>, String)> {
        let Inner::Local { root, .. } = &*self.inner else {
            return None;
        };
        let key = sanitize_key(key).ok()?;
        let path = root.join(&key);
        let bytes = std::fs::read(path).ok()?;
        Some((bytes, guess_type(&key, "")))
    }
}

struct R2Settings {
    access_key: String,
    secret_key: String,
    bucket: String,
    endpoint: String,
    public_base: String,
}

impl R2Settings {
    fn from_env() -> Result<Option<Self>, String> {
        let access_key = first_env(&["R2_ACCESS_KEY_ID", "S3_ACCESS_KEY_ID"]);
        let secret_key = first_env(&["R2_SECRET_ACCESS_KEY", "S3_SECRET_ACCESS_KEY"]);
        let bucket = first_env(&["R2_BUCKET", "CLOUDFLARE_BUCKET_NAME"]);
        match (access_key, secret_key, bucket) {
            (None, None, None) => Ok(None),
            (Some(access_key), Some(secret_key), Some(bucket)) => {
                let account = first_env(&["R2_ACCOUNT_ID", "CLOUDFLARE_ACCOUNT_ID"]);
                let endpoint = first_env(&["R2_ENDPOINT"]).unwrap_or_else(|| {
                    account
                        .as_deref()
                        .map(|id| format!("https://{id}.r2.cloudflarestorage.com"))
                        .unwrap_or_default()
                });
                if endpoint.is_empty() {
                    return Err("set R2_ENDPOINT or R2_ACCOUNT_ID".into());
                }
                let public_base = first_env(&["R2_PUBLIC_URL", "R2_PUBLIC_BASE"])
                    .ok_or("set R2_PUBLIC_URL (public bucket or custom domain)")?;
                Ok(Some(Self {
                    access_key,
                    secret_key,
                    bucket,
                    endpoint,
                    public_base,
                }))
            }
            _ => Err("R2 needs access key, secret, and bucket".into()),
        }
    }
}

fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn sanitize_key(key: &str) -> Result<String, String> {
    let key = key.trim().trim_start_matches('/');
    if key.is_empty() {
        return Err("empty key".into());
    }
    if key.contains("..") {
        return Err("key must not contain ..".into());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        return Err("key has a character outside A-Za-z0-9._/-".into());
    }
    Ok(key.to_string())
}

pub fn safe_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("exhibit");
    let s: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').trim_matches('.');
    if s.is_empty() {
        "exhibit".into()
    } else {
        s.chars().take(80).collect()
    }
}

pub fn slug_id(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let mut s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s: String = s.trim_matches('-').chars().take(64).collect();
    if s.is_empty() {
        "exhibit".into()
    } else {
        s
    }
}

pub fn guess_type(name: &str, provided: &str) -> String {
    let provided = provided.trim();
    if !provided.is_empty() && provided != "application/octet-stream" {
        return provided.to_string();
    }
    match Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
    .into()
}

pub fn looks_like_image(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_put_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::local(dir.path().join("blobs"), "http://court.test");
        let href = store
            .put("cases/case-1/snap/stv-snap.png", b"png-bytes", "image/png")
            .await
            .unwrap();
        assert_eq!(
            href,
            "http://court.test/blobs/cases/case-1/snap/stv-snap.png"
        );
        let (got, ct) = store
            .get_local("cases/case-1/snap/stv-snap.png")
            .expect("bytes");
        assert_eq!(got, b"png-bytes");
        assert_eq!(ct, "image/png");
    }

    #[test]
    fn rejects_dotdot_keys() {
        assert!(sanitize_key("../etc/passwd").is_err());
        assert!(sanitize_key("cases/ok/file.png").is_ok());
    }

    #[test]
    fn slugs_filenames() {
        assert_eq!(slug_id("STV snap.PNG"), "stv-snap");
        assert_eq!(safe_filename("STV snap.PNG"), "STV-snap.PNG");
        assert!(looks_like_image("stv-snap.png"));
    }
}
