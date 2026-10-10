use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub system: String,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub text: String,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider is not configured")]
    NotConfigured,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(&self, request: AiRequest) -> Result<AiResponse, AiError>;
}

pub struct DisabledProvider;

#[async_trait]
impl AiProvider for DisabledProvider {
    async fn complete(&self, _request: AiRequest) -> Result<AiResponse, AiError> {
        Err(AiError::NotConfigured)
    }
}

pub struct OpenRouterProvider {
    pub api_key: String,
    pub model: String,
    client: reqwest::Client,
}

impl OpenRouterProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AiProvider for OpenRouterProvider {
    async fn complete(&self, request: AiRequest) -> Result<AiResponse, AiError> {
        let body = serde_json::json!({"model": self.model, "messages": [{"role":"system","content":request.system},{"role":"user","content":request.user}]});
        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Request(e.to_string()))?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AiError::Request(e.to_string()))?;
        let text = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(AiResponse {
            text,
            model: self.model.clone(),
        })
    }
}
