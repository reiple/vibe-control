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
    /// Local calendar date (YYYY-MM-DD) the daily usage counters below belong
    /// to. The frontend supplies it (it owns the local timezone); a new date
    /// rolls the counters back to zero. Purely a UI readout — token counts only,
    /// never any prompt/response content.
    #[serde(default)]
    pub usage_date: Option<String>,
    /// Today's cumulative Bedrock input tokens.
    #[serde(default)]
    pub usage_input: u64,
    /// Today's cumulative Bedrock output tokens.
    #[serde(default)]
    pub usage_output: u64,
    /// Today's number of Bedrock calls.
    #[serde(default)]
    pub usage_requests: u64,
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
            usage_date: None,
            usage_input: 0,
            usage_output: 0,
            usage_requests: 0,
        }
    }
}
