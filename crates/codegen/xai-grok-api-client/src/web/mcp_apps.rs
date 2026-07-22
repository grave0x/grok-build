use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// MCP App endpoints (call tools, list resources, read resources).
impl HttpClient {
    /// Call an MCP tool.
    /// POST /api/mcp-app/call-tool
    pub async fn web_mcp_call_tool(
        &self,
        req: McpToolCallRequest,
    ) -> Result<McpToolCallResponse, ApiError> {
        let url = format!("{}/api/mcp-app/call-tool", self.config().web_base_url);
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }

    /// List available MCP resources.
    /// POST /api/mcp-app/list-resources
    pub async fn web_mcp_list_resources(&self) -> Result<Vec<McpResource>, ApiError> {
        let url = format!("{}/api/mcp-app/list-resources", self.config().web_base_url);
        let req = serde_json::json!({});
        let resp = self.post_json(&url, &req).await?;
        let wrapped: ApiResponse<Vec<McpResource>> = self.json(resp).await?;
        Ok(wrapped.result)
    }

    /// List available MCP resource templates.
    /// POST /api/mcp-app/list-resource-templates
    pub async fn web_mcp_list_resource_templates(
        &self,
    ) -> Result<Vec<McpResourceTemplate>, ApiError> {
        let url = format!(
            "{}/api/mcp-app/list-resource-templates",
            self.config().web_base_url
        );
        let req = serde_json::json!({});
        let resp = self.post_json(&url, &req).await?;
        let wrapped: ApiResponse<Vec<McpResourceTemplate>> = self.json(resp).await?;
        Ok(wrapped.result)
    }

    /// Read an MCP resource by URI.
    /// POST /api/mcp-app/read-resource
    pub async fn web_mcp_read_resource(
        &self,
        req: McpReadResourceRequest,
    ) -> Result<McpReadResourceResponse, ApiError> {
        let url = format!(
            "{}/api/mcp-app/read-resource",
            self.config().web_base_url
        );
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }
}
