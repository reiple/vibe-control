use crate::models::ResourceIdentity;
use super::MatchResult;

/// 실행 중 창/탭 스냅샷
#[derive(Clone, Debug)]
pub struct RunningItem {
    pub app_id: String,
    pub role: Option<String>,
    pub title: String,
    pub is_focused: bool,
}

/// 계층 매칭: 강한 키 필수 일치, 제목은 보조
pub fn matches(saved: &ResourceIdentity, running: &RunningItem) -> MatchResult {
    // 1) 강한 키: 앱 ID + 역할 필수 일치
    let saved_app = extract_app_id(&saved.descriptor);
    if saved_app != running.app_id {
        return MatchResult::NoMatch;
    }

    let saved_role = extract_role(&saved.descriptor);
    if let Some(expected_role) = saved_role {
        if running.role.as_deref() != Some(expected_role) {
            return MatchResult::NoMatch;
        }
    }

    // 2) 강한 키 일치 → 제목으로 tie-break (보조)
    let title_sig = normalize_title(&running.title);
    let saved_title_sig = normalize_title(extract_title(&saved.descriptor).unwrap_or(""));

    if title_sig == saved_title_sig {
        MatchResult::Match
    } else if is_focused_or_default(running) {
        // 포커스나 기본값이면 Match로 처리 가능
        MatchResult::Match
    } else {
        // 모호: 다중 후보 가능
        MatchResult::Ambiguous
    }
}

fn extract_app_id(descriptor: &str) -> String {
    descriptor.split('|').next().unwrap_or(descriptor).to_string()
}

fn extract_role(descriptor: &str) -> Option<&str> {
    descriptor.split('|').nth(1)
}

fn extract_title(descriptor: &str) -> Option<&str> {
    descriptor.split('|').nth(2)
}

fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase()
}

fn is_focused_or_default(running: &RunningItem) -> bool {
    running.is_focused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strong_key_mismatch() {
        let saved = ResourceIdentity {
            kind: crate::models::ResourceKind::WindowRef,
            descriptor: "com.example.app|MainWindow|Title".to_string(),
            hint: None,
            reopen_info: None,
        };

        let running = RunningItem {
            app_id: "com.other.app".to_string(),
            role: Some("MainWindow".to_string()),
            title: "Title".to_string(),
            is_focused: true,
        };

        assert_eq!(matches(&saved, &running), MatchResult::NoMatch);
    }

    #[test]
    fn test_strong_key_match_title_match() {
        let saved = ResourceIdentity {
            kind: crate::models::ResourceKind::WindowRef,
            descriptor: "com.example.app|MainWindow|My Window".to_string(),
            hint: None,
            reopen_info: None,
        };

        let running = RunningItem {
            app_id: "com.example.app".to_string(),
            role: Some("MainWindow".to_string()),
            title: "My Window".to_string(),
            is_focused: false,
        };

        assert_eq!(matches(&saved, &running), MatchResult::Match);
    }
}
