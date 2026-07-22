use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Skills listing endpoints.
impl HttpClient {
    /// List available skills.
    /// POST /rest/skills
    pub async fn web_list_skills(&self) -> Result<Vec<SkillInfo>, ApiError> {
        let url = format!("{}/rest/skills", self.config().web_base_url);
        let resp = self.post_json(&url, &serde_json::json!({})).await?;
        let wrapped: ApiResponse<Vec<SkillInfo>> = self.json(resp).await?;
        Ok(wrapped.result)
    }
}
