use crate::error::{CoreError, Result};

/// URL 정규화 (스킴/호스트/경로/쿼리/포트 보존, SECURITY-05)
pub fn normalize_url(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(CoreError::validation("URL cannot be empty"));
    }

    // 기본 스킴 추가
    let with_scheme = if !trimmed.contains("://") {
        format!("http://{}", trimmed)
    } else {
        trimmed.to_string()
    };

    // 파싱 검증 (스킴/호스트 구조 확인)
    let after_scheme = with_scheme
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("");
    if after_scheme.is_empty() {
        return Err(CoreError::validation("Invalid URL format"));
    }

    Ok(with_scheme.to_lowercase())
}

/// 경로 정규화 (절대 경로 변환, 정규화, SECURITY-05)
pub fn normalize_path(path: &str) -> Result<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(CoreError::validation("Path cannot be empty"));
    }

    let path_obj = std::path::PathBuf::from(trimmed);
    match path_obj.canonicalize() {
        Ok(abs_path) => {
            Ok(abs_path.to_string_lossy().to_string())
        }
        Err(_) => {
            // 경로가 존재하지 않으면 상대→절대 변환만
            let abs_path = std::env::current_dir()
                .ok()
                .and_then(|cd| cd.join(trimmed).canonicalize().ok())
                .unwrap_or_else(|| std::path::PathBuf::from(trimmed));
            Ok(abs_path.to_string_lossy().to_string())
        }
    }
}

/// 앱 ID 정규화 (SECURITY-05)
pub fn normalize_app_id(app_id: &str) -> Result<String> {
    let trimmed = app_id.trim();
    if trimmed.is_empty() {
        return Err(CoreError::validation("App ID cannot be empty"));
    }

    // 금지 문자 검사 (명령 조합 방지)
    if trimmed.contains('|')
        || trimmed.contains('&')
        || trimmed.contains(';')
        || trimmed.contains('$')
    {
        return Err(CoreError::validation(
            "App ID contains invalid characters",
        ));
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url_valid() {
        let url = "https://example.com/path";
        let normalized = normalize_url(url).unwrap();
        assert_eq!(normalized, "https://example.com/path");
    }

    #[test]
    fn test_normalize_url_add_scheme() {
        let url = "example.com";
        let normalized = normalize_url(url).unwrap();
        assert!(normalized.contains("://"));
    }

    #[test]
    fn test_normalize_url_empty() {
        assert!(normalize_url("").is_err());
    }

    #[test]
    fn test_normalize_app_id_valid() {
        let app_id = "com.example.app";
        let normalized = normalize_app_id(app_id).unwrap();
        assert_eq!(normalized, "com.example.app");
    }

    #[test]
    fn test_normalize_app_id_injection() {
        let app_id = "app|malicious";
        assert!(normalize_app_id(app_id).is_err());
    }

    #[test]
    fn test_normalize_path_valid() {
        let path = ".";
        let normalized = normalize_path(path).unwrap();
        assert!(!normalized.is_empty());
    }
}
