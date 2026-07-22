use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Task usage/quota endpoints.
impl HttpClient {
    /// Get task usage and quota information.
    /// GET /rest/tasks
    pub async fn web_get_task_usage(&self) -> Result<TasksUsage, ApiError> {
        let url = format!("{}/rest/tasks", self.config().web_base_url);
        let resp = self.get(&url).await?;
        let wrapped: ApiResponse<TasksUsage> = self.json(resp).await?;
        Ok(wrapped.result)
    }
}
