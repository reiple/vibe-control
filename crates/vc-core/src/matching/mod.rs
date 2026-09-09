use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::models::{ResourceIdentity, ResourceKind};

pub mod window;

/// 안정 식별 시그니처 (MatchSignature)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MatchSignature {
    pub kind: ResourceKind,
    pub hash: u64,
}

/// 매칭 결과
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchResult {
    Match,
    NoMatch,
    Ambiguous,
}

/// MatchSignature 계산 (결정적, 안정 서술자만)
pub fn match_signature(id: &ResourceIdentity) -> MatchSignature {
    let descriptor = normalize_descriptor(&id.descriptor, id.kind);

    let mut hasher = DefaultHasher::new();
    id.kind.hash(&mut hasher);
    descriptor.hash(&mut hasher);
    let hash = hasher.finish();

    MatchSignature {
        kind: id.kind,
        hash,
    }
}

/// 유사 항목 구분 키
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistinctKey {
    pub kind: ResourceKind,
    pub key: String,
}

/// 유사 항목 구분 (같은 앱/URL/경로 여부)
pub fn distinct_key(id: &ResourceIdentity) -> DistinctKey {
    let key = match id.kind {
        ResourceKind::WindowRef => id.descriptor.clone(),
        ResourceKind::BrowserTab => extract_host_path(&id.descriptor),
        // A live tab has no URL (FR-9.7): distinguish tabs by their activation
        // token (browser+window+index) when present, else by title.
        ResourceKind::BrowserTabLive => {
            id.hint.clone().unwrap_or_else(|| id.descriptor.clone())
        }
        ResourceKind::Folder => id.descriptor.clone(),
        ResourceKind::AppLaunch => id.descriptor.clone(),
        ResourceKind::Url => extract_host_path(&id.descriptor),
        ResourceKind::CodingSession => id.descriptor.clone(),
    };

    DistinctKey {
        kind: id.kind,
        key,
    }
}

fn normalize_descriptor(desc: &str, kind: ResourceKind) -> String {
    match kind {
        ResourceKind::BrowserTab | ResourceKind::Url => {
            // URL 정규화: 스킴/호스트/경로/쿼리/포트만 유지
            desc.to_lowercase()
        }
        _ => desc.to_string(),
    }
}

fn extract_host_path(url: &str) -> String {
    // 간단한 호스트+경로 추출
    if let Some(proto_end) = url.find("://") {
        let after_proto = &url[proto_end + 3..];
        if let Some(path_start) = after_proto.find('/') {
            let host = &after_proto[..path_start];
            let path = &after_proto[path_start..];
            format!("{}{}", host, path)
        } else {
            after_proto.to_string()
        }
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_sig_deterministic() {
        let id = ResourceIdentity {
            kind: ResourceKind::Url,
            descriptor: "https://example.com/path".to_string(),
            hint: None,
            reopen_info: None,
        };

        let sig1 = match_signature(&id);
        let sig2 = match_signature(&id);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_distinct_urls_same_host() {
        let id1 = ResourceIdentity {
            kind: ResourceKind::BrowserTab,
            descriptor: "https://example.com/page1".to_string(),
            hint: None,
            reopen_info: None,
        };

        let id2 = ResourceIdentity {
            kind: ResourceKind::BrowserTab,
            descriptor: "https://example.com/page2".to_string(),
            hint: None,
            reopen_info: None,
        };

        let key1 = distinct_key(&id1);
        let key2 = distinct_key(&id2);
        // 같은 호스트면 동일 경로까지 제외 가능 (설계에 따라)
        assert_ne!(key1, key2); // 경로까지 포함하면 다름
    }

    // FR-3.4: 라이브 탭은 URL이 없으므로(FR-9.7) 활성화 토큰(hint =
    // <browser>\u{1f}<hwnd>\u{1f}<idx>)으로 구분된다 — 같은 탭이면 동일 키(중복),
    // 다른 탭(다른 인덱스)이면 다른 키(개별 등록 허용).
    #[test]
    fn test_distinct_live_tabs_by_hint() {
        let same_title = "GitHub - reiple/vibe-control".to_string();
        let tab0 = ResourceIdentity {
            kind: ResourceKind::BrowserTabLive,
            descriptor: same_title.clone(),
            hint: Some("chrome\u{1f}12345\u{1f}0".to_string()),
            reopen_info: None,
        };
        let tab0_again = ResourceIdentity {
            kind: ResourceKind::BrowserTabLive,
            descriptor: same_title.clone(),
            hint: Some("chrome\u{1f}12345\u{1f}0".to_string()),
            reopen_info: None,
        };
        let tab1 = ResourceIdentity {
            kind: ResourceKind::BrowserTabLive,
            descriptor: same_title,
            hint: Some("chrome\u{1f}12345\u{1f}1".to_string()),
            reopen_info: None,
        };
        // 동일 탭 → 동일 키 (묶음 내 중복 방지 근거)
        assert_eq!(distinct_key(&tab0), distinct_key(&tab0_again));
        // 같은 제목이라도 인덱스가 다르면 별개 탭 → 다른 키 (개별 등록 허용)
        assert_ne!(distinct_key(&tab0), distinct_key(&tab1));
    }
}
