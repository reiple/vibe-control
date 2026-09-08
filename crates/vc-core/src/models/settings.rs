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
    /// Explicit user consent to send Claude Code session content to an external
    /// service for work summarization (§12 / NFR-4). Defaults to `false`: without
    /// opt-in, summaries degrade to `insufficient_information` rather than making
    /// any external call. `#[serde(default)]` so older settings.json still loads.
    #[serde(default)]
    pub session_summary_consent: bool,
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
            session_summary_consent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P12 / AC-18 / NFR-5: settings.json written by an older build (without the
    /// new fields) must still load, filling the new fields from `#[serde(default)]`.
    #[test]
    fn loads_legacy_settings_without_new_fields() {
        let legacy = r#"{
            "panel_width": 320.0,
            "card_height": 160.0,
            "window_x": null,
            "window_y": null,
            "window_width": null,
            "window_height": null
        }"#;
        let s: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(s.panel_width, 320.0);
        // New fields default when absent.
        assert_eq!(s.session_summary_consent, false);
        assert!(s.claude_api_key.is_none());
    }

    #[test]
    fn roundtrips_with_consent() {
        let mut s = AppSettings::default();
        s.session_summary_consent = true;
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(back.session_summary_consent);
    }
}
