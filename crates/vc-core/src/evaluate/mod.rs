use crate::models::{Resource, ResourceStatus};

/// 상태 스냅샷
#[derive(Clone, Debug)]
pub struct StatusSnapshot {
    pub resource_id: crate::models::ResourceId,
    pub status: ResourceStatus,
}

/// 상태 판정 (우선순위 규칙: PermissionRequired > Active > Inactive > Unknown)
pub fn evaluate_status(
    res: &Resource,
    is_running: bool,
    has_permission: bool,
    session_available: bool,
) -> ResourceStatus {
    if !has_permission {
        ResourceStatus::PermissionRequired
    } else if is_running {
        ResourceStatus::Active
    } else if res.identity.reopen_info.is_some() || session_available {
        ResourceStatus::Inactive
    } else {
        ResourceStatus::Unknown
    }
}

/// 노이즈 판정 (보조/시스템 창 제외)
pub fn is_noise(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.contains("system")
        || lower.contains("auxiliary")
        || lower.contains("muted")
        || lower.contains("settings")
}

/// 전체 평가
pub fn evaluate(
    resources: &[Resource],
    running_titles: &[String],
    has_permission: bool,
) -> Vec<StatusSnapshot> {
    resources
        .iter()
        .map(|res| {
            let is_running = running_titles
                .iter()
                .any(|title| title == &res.identity.descriptor && !is_noise(title));
            let status = evaluate_status(res, is_running, has_permission, false);

            StatusSnapshot {
                resource_id: res.id,
                status,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_permission_required() {
        let res = Resource::new(
            "Test",
            crate::models::ResourceKind::Url,
            crate::models::ResourceIdentity {
                kind: crate::models::ResourceKind::Url,
                descriptor: "https://example.com".to_string(),
                hint: None,
                reopen_info: Some("https://example.com".to_string()),
            },
        );

        let status = evaluate_status(&res, true, false, false);
        assert_eq!(status, ResourceStatus::PermissionRequired);
    }

    #[test]
    fn test_is_noise() {
        assert!(is_noise("System Settings"));
        assert!(is_noise("Muted Tab"));
        assert!(!is_noise("My Regular Window"));
    }

    fn res_with_descriptor(descriptor: &str) -> Resource {
        Resource::new(
            descriptor,
            crate::models::ResourceKind::WindowRef,
            crate::models::ResourceIdentity {
                kind: crate::models::ResourceKind::WindowRef,
                descriptor: descriptor.to_string(),
                hint: None,
                reopen_info: None,
            },
        )
    }

    // FR-7.1/7.2: the exact path get_bundles wires — a resource whose descriptor
    // matches a currently-running title is Active; one that doesn't is Unknown
    // (no reopen_info/session), and evaluation preserves resource order/id.
    #[test]
    fn test_evaluate_active_when_running() {
        let a = res_with_descriptor("Obsidian");
        let b = res_with_descriptor("SomeClosedApp");
        let resources = vec![a.clone(), b.clone()];
        let running = vec!["Obsidian".to_string(), "제목 없음 - 그림판".to_string()];

        let snaps = evaluate(&resources, &running, true);
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].resource_id, a.id);
        assert_eq!(snaps[0].status, ResourceStatus::Active);
        assert_eq!(snaps[1].resource_id, b.id);
        assert_eq!(snaps[1].status, ResourceStatus::Unknown);
    }

    // A running title that is "noise" (system/aux window) must NOT count as a
    // match, so a resource named after it stays non-Active.
    #[test]
    fn test_evaluate_noise_title_does_not_match() {
        let res = res_with_descriptor("System Settings");
        let running = vec!["System Settings".to_string()];
        let snaps = evaluate(&[res], &running, true);
        assert_ne!(snaps[0].status, ResourceStatus::Active);
    }
}
