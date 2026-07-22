use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// File management endpoints (read, create directories, move, upload).
impl HttpClient {
    /// Read a file's content by path.
    /// GET /files/content?path=...
    pub async fn web_file_content(&self, path: &str) -> Result<FileContent, ApiError> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/files/content?path={}", self.config().web_base_url, encoded);
        let resp = self.get(&url).await?;
        self.json(resp).await
    }

    /// Create a directory at the given path.
    /// POST /files/mkdir
    pub async fn web_file_mkdir(&self, path: &str) -> Result<(), ApiError> {
        let url = format!("{}/files/mkdir", self.config().web_base_url);
        let req = MkdirRequest {
            path: path.to_string(),
        };
        let resp = self.post_json(&url, &req).await?;
        let _: serde_json::Value = self.json(resp).await?;
        Ok(())
    }

    /// Move a file or directory from source to destination.
    /// POST /files/move
    pub async fn web_file_move(&self, source: &str, destination: &str) -> Result<(), ApiError> {
        let url = format!("{}/files/move", self.config().web_base_url);
        let req = MoveRequest {
            source: source.to_string(),
            destination: destination.to_string(),
        };
        let resp = self.post_json(&url, &req).await?;
        let _: serde_json::Value = self.json(resp).await?;
        Ok(())
    }

    /// Upload a file as multipart form data.
    /// POST /files/upload
    pub async fn web_file_upload(
        &self,
        path: &str,
        content_type: &str,
        data: bytes::Bytes,
    ) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}/files/upload", self.config().web_base_url);
        let file_name = path.rsplit('/').next().unwrap_or(path).to_string();
        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(file_name)
            .mime_str(content_type)
            .map_err(|e| ApiError::Other(format!("Invalid mime type: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .text("path", path.to_string())
            .part("file", part);

        let mut builder = self.inner().post(&url).multipart(form);
        if let Some(auth) = self.auth() {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        crate::client::check_status_internal(&resp).await?;
        self.json(resp).await
    }

    /// Upload a file as raw bytes (alternative to multipart).
    /// POST /files/upload?path=...
    pub async fn web_file_upload_raw(
        &self,
        path: &str,
        content_type: &str,
        data: bytes::Bytes,
    ) -> Result<serde_json::Value, ApiError> {
        let encoded = urlencoding::encode(path);
        let url = format!(
            "{}/files/upload?path={}",
            self.config().web_base_url,
            encoded
        );
        let resp = self.post_bytes(&url, content_type, data).await?;
        self.json(resp).await
    }
}
