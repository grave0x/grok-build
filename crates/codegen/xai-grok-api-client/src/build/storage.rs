use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Storage API endpoints (signed upload URLs, file confirmation, batch operations).
impl HttpClient {
    /// Get a pre-signed GCS upload URL for a file.
    pub async fn storage_get_signed_upload_url(
        &self,
        req: SignedUploadUrlRequest,
    ) -> Result<SignedUploadUrlResponse, ApiError> {
        let url = format!(
            "{}/v1/storage/signed-upload-url",
            self.config().build_base_url
        );
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }

    /// Confirm that a file upload to a storage provider has completed.
    pub async fn storage_confirm_upload(
        &self,
        req: ConfirmUploadRequest,
    ) -> Result<ConfirmUploadResponse, ApiError> {
        let url = format!("{}/v1/storage", self.config().build_base_url);
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }

    /// Upload raw bytes to a pre-signed URL (direct HTTP PUT to GCS).
    pub async fn storage_upload_bytes(
        &self,
        upload_url: &str,
        content_type: &str,
        data: bytes::Bytes,
    ) -> Result<(), ApiError> {
        let client = reqwest::Client::new();
        let resp = client
            .put(upload_url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(data)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status < 200 || status >= 300 {
            let message = resp.text().await.unwrap_or_default();
            return Err(ApiError::Http { status, message });
        }
        Ok(())
    }

    /// Batch-upload multiple files encoded as base64+zstd JSON.
    pub async fn storage_batch_upload_json(
        &self,
        entries: Vec<BatchUploadEntry>,
    ) -> Result<BatchUploadResponse, ApiError> {
        let url = format!(
            "{}/v1/storage/batch_upload_json",
            self.config().build_base_url
        );

        // Encode entries as JSON
        let json = serde_json::to_string(&entries)?;

        // Compress with zstd
        let compressed = zstd::encode_all(std::io::Cursor::new(json.as_bytes()), 3)
            .map_err(|e| ApiError::Other(format!("Zstd compression failed: {e}")))?;

        let resp = self
            .post_bytes(&url, "application/zstd", compressed.into())
            .await?;
        self.json(resp).await
    }

    /// Check which file paths already exist in storage.
    pub async fn storage_check_exists(
        &self,
        paths: &[String],
    ) -> Result<CheckExistsResponse, ApiError> {
        let encoded: Vec<String> = paths
            .iter()
            .map(|p| urlencoding::encode(p).into_owned())
            .collect();
        let query = encoded.join(",");
        let url = format!(
            "{}/v1/storage/exists?paths={}",
            self.config().build_base_url,
            query
        );
        let resp = self.get(&url).await?;
        self.json(resp).await
    }
}
