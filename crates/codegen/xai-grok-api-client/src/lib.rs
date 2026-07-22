pub mod auth;
pub mod build;
pub mod client;
pub mod error;
pub mod types;
pub mod web;

pub use auth::{ApiAuth, BearerAuth, NoAuth, SsoAuth};
pub use client::{ClientConfig, HttpClient};
pub use error::ApiError;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parse_empty() {
        let result = crate::build::chat::parse_sse_chunks("");
        assert!(result.is_empty());
    }

    #[test]
    fn sse_parse_single_chunk() {
        let chunk = r#"data: {"id":"1","object":"chat.completion.chunk","created":123,"model":"grok","choices":[{"index":0,"delta":{"role":"assistant","content":"hello"},"finish_reason":null}]}"#;
        let results = crate::build::chat::parse_sse_chunks(chunk);
        assert_eq!(results.len(), 1);
        let parsed = results[0].as_ref().unwrap();
        assert_eq!(parsed.choices[0].delta.content.as_deref(), Some("hello"));
    }

    #[test]
    fn sse_parse_done() {
        let input = "data: [DONE]";
        let results = crate::build::chat::parse_sse_chunks(input);
        assert!(results.is_empty());
    }

    #[test]
    fn sse_parse_ignores_comments() {
        let input = ": comment\ndata: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":123,\"model\":\"grok\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"},\"finish_reason\":null}]}";
        let results = crate::build::chat::parse_sse_chunks(input);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn sse_parse_json_events() {
        let input = "data: {\"type\":\"ping\"}\ndata: {\"type\":\"pong\"}";
        let results = crate::build::chat::parse_sse_json_events(input);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn client_config_default() {
        let c = ClientConfig::default();
        assert_eq!(c.build_base_url, "https://cli-chat-proxy.grok.com");
        assert_eq!(c.web_base_url, "https://grok.com");
        assert_eq!(c.timeout.as_secs(), 120);
    }

    #[test]
    fn new_bearer_sets_timeout() {
        let client = HttpClient::new_bearer("tok".into(), 30);
        assert_eq!(client.config().timeout.as_secs(), 30);
    }

    #[test]
    fn new_sso_sets_timeout() {
        let client = HttpClient::new_sso("cookie".into(), 45);
        assert_eq!(client.config().timeout.as_secs(), 45);
    }

    #[test]
    fn api_error_display() {
        let err = ApiError::Http { status: 401, message: "Unauthorized".into() };
        let msg = format!("{err}");
        assert!(msg.contains("401"));
    }

    #[test]
    fn chat_completion_request_serde() {
        let req = ChatCompletionRequest {
            model: "grok-3".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: Some(100),
            temperature: Some(0.5),
            stream: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ChatCompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "grok-3");
        assert_eq!(back.messages.len(), 1);
    }
}
