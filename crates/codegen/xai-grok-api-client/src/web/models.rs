use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Model listing endpoints.
impl HttpClient {
    /// List available models.
    /// POST /rest/models
    pub async fn web_list_models(&self) -> Result<Vec<ModelInfo>, ApiError> {
        let url = format!("{}/rest/models", self.config().web_base_url);
        let resp = self.post_json(&url, &serde_json::json!({})).await?;
        let wrapped: ApiResponse<Vec<ModelInfo>> = self.json(resp).await?;
        Ok(wrapped.result)
    }
}
