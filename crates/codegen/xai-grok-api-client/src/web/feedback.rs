use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Feedback submission endpoints.
impl HttpClient {
    /// Submit feedback.
    /// POST /api/feedback
    pub async fn web_submit_feedback(&self, req: FeedbackRequest) -> Result<(), ApiError> {
        let url = format!("{}/api/feedback", self.config().web_base_url);
        let resp = self.post_json(&url, &req).await?;
        let _: serde_json::Value = self.json(resp).await?;
        Ok(())
    }
}
