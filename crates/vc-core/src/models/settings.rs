use serde::{Deserialize, Serialize};

/// 애플리케이션 설정 (UI/레이아웃)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub panel_width: f32,
    pub card_height: f32,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
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
        }
    }
}
