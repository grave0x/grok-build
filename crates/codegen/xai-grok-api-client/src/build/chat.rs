use futures_util::StreamExt;

use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Chat completions via the Grok Build API (OpenAI-compatible).
impl HttpClient {
    /// Send a chat completion request (non-streaming).
    pub async fn build_chat_completion(
        &self,
        req: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ApiError> {
        let url = format!("{}/v1/chat/completions", self.config().build_base_url);
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }

    /// Send a streaming chat completion request.
    /// Returns a stream of `StreamChunk` events.
    pub async fn build_chat_completion_stream(
        &self,
        req: ChatCompletionRequest,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamChunk, ApiError>>, ApiError> {
        let url = format!("{}/v1/chat/completions", self.config().build_base_url);
        let mut builder = self.inner().post(&url).json(&req);
        if let Some(auth) = self.auth() {
            builder = auth.apply(builder).await;
        }
        let response = builder.send().await?;

        let stream = response.bytes_stream().flat_map(|chunk| {
            let results: Vec<Result<StreamChunk, ApiError>> = match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    parse_sse_chunks(&text)
                }
                Err(e) => vec![Err(ApiError::Request(e))],
            };
            futures_util::stream::iter(results)
        });
        Ok(stream)
    }

    /// Send an Anthropic-compatible messages request (non-streaming).
    pub async fn build_anthropic_messages(
        &self,
        req: AnthropicRequest,
    ) -> Result<AnthropicResponse, ApiError> {
        let url = format!("{}/v1/messages", self.config().build_base_url);
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }

    /// Send an Anthropic-compatible messages request (streaming).
    pub async fn build_anthropic_messages_stream(
        &self,
        req: AnthropicRequest,
    ) -> Result<impl futures_util::Stream<Item = Result<serde_json::Value, ApiError>>, ApiError> {
        let url = format!("{}/v1/messages", self.config().build_base_url);
        let mut builder = self.inner().post(&url).json(&req);
        if let Some(auth) = self.auth() {
            builder = auth.apply(builder).await;
        }
        let response = builder.send().await?;

        let stream = response.bytes_stream().flat_map(|chunk| {
            let results: Vec<Result<serde_json::Value, ApiError>> = match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    parse_sse_json_events(&text)
                }
                Err(e) => vec![Err(ApiError::Request(e))],
            };
            futures_util::stream::iter(results)
        });
        Ok(stream)
    }
}

pub fn parse_sse_chunks(text: &str) -> Vec<Result<StreamChunk, ApiError>> {
    let mut results = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }
            match serde_json::from_str::<StreamChunk>(data) {
                Ok(chunk) => results.push(Ok(chunk)),
                Err(e) => results.push(Err(ApiError::Serialization(e))),
            }
        }
    }
    results
}

pub fn parse_sse_json_events(text: &str) -> Vec<Result<serde_json::Value, ApiError>> {
    let mut results = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }
            match serde_json::from_str::<serde_json::Value>(data) {
                Ok(val) => results.push(Ok(val)),
                Err(e) => results.push(Err(ApiError::Serialization(e))),
            }
        }
    }
    results
}
