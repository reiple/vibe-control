use serde::{Deserialize, Serialize};

/// 애플리케이션 설정 (UI/레이아웃 + Claude 연동)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub panel_width: f32,
    pub card_height: f32,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    /// Bedrock bearer token (AWS Bedrock API key) for the in-app Claude prompt
    /// console. Stored locally only (this file lives in the OS config dir, never
    /// the repo). `#[serde(default)]` so settings.json written by older builds
    /// still loads.
    #[serde(default)]
    pub claude_api_key: Option<String>,
    /// Preferred Bedrock model id (e.g. "global.anthropic.claude-opus-4-8");
    /// None → app default.
    #[serde(default)]
    pub claude_model: Option<String>,
    /// AWS region for the Bedrock endpoint (e.g. "ap-northeast-2"); None → env
    /// `AWS_REGION` or the app default.
    #[serde(default)]
    pub claude_region: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            panel_width: 300.0,
            card_height: 150.0,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            claude_api_key: None,
            claude_model: None,
            claude_region: None,
        }
    }
}
