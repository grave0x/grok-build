use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Workspace management endpoints.
impl HttpClient {
    /// List all workspaces.
    /// GET /rest/workspaces
    pub async fn web_list_workspaces(&self) -> Result<Vec<Workspace>, ApiError> {
        let url = format!("{}/rest/workspaces", self.config().web_base_url);
        let resp = self.get(&url).await?;
        let wrapped: ApiResponse<Vec<Workspace>> = self.json(resp).await?;
        Ok(wrapped.result)
    }

    /// Create a new workspace.
    /// POST /rest/workspaces
    pub async fn web_create_workspace(&self, name: &str) -> Result<Workspace, ApiError> {
        let url = format!("{}/rest/workspaces", self.config().web_base_url);
        let req = WorkspaceCreateRequest {
            name: name.to_string(),
        };
        let resp = self.post_json(&url, &req).await?;
        let wrapped: ApiResponse<Workspace> = self.json(resp).await?;
        Ok(wrapped.result)
    }
}
