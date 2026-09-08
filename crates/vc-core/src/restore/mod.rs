use crate::models::{Resource, ResourceKind, WorkBundle};

/// 재실행 액션
#[derive(Clone, Debug)]
pub enum ReopenAction {
    LaunchApp { app_id: String },
    OpenPath { path: String },
    OpenUrl { url: String },
    FocusLinkedWindow,
    ShowSessionDetail { session_ref: String },
}

/// 활성화 스텝
#[derive(Clone, Debug)]
pub struct ActivationStep {
    pub resource_id: crate::models::ResourceId,
    pub action: ReopenAction,
}

/// 단일 리소스 재실행 계획
pub fn plan_reopen(res: &Resource) -> ReopenAction {
    match res.kind {
        ResourceKind::WindowRef => {
            if let Some(reopen) = &res.identity.reopen_info {
                ReopenAction::OpenPath {
                    path: reopen.clone(),
                }
            } else {
                let app_id = res.identity.descriptor.split('|').next().unwrap_or("").to_string();
                ReopenAction::LaunchApp { app_id }
            }
        }
        ResourceKind::BrowserTab => {
            let url = res.identity.reopen_info.as_deref().unwrap_or(&res.identity.descriptor);
            ReopenAction::OpenUrl {
                url: url.to_string(),
            }
        }
        // A live tab has no URL to open (FR-9.7) — restore by re-focusing the
        // tab in its running browser, never by launching a URL.
        ResourceKind::BrowserTabLive => ReopenAction::FocusLinkedWindow,
        ResourceKind::Folder => {
            let path = res.identity.reopen_info.as_deref().unwrap_or(&res.identity.descriptor);
            ReopenAction::OpenPath {
                path: path.to_string(),
            }
        }
        ResourceKind::AppLaunch => {
            let app_id = res.identity.reopen_info.as_deref().unwrap_or(&res.identity.descriptor);
            ReopenAction::LaunchApp {
                app_id: app_id.to_string(),
            }
        }
        ResourceKind::Url => {
            let url = res.identity.reopen_info.as_deref().unwrap_or(&res.identity.descriptor);
            ReopenAction::OpenUrl {
                url: url.to_string(),
            }
        }
        ResourceKind::CodingSession => ReopenAction::FocusLinkedWindow,
    }
}

/// 묶음 전체 활성화 계획 (정렬 순서 보존)
pub fn plan_bundle_activation(bundle: &WorkBundle) -> Vec<ActivationStep> {
    bundle
        .resources
        .iter()
        .map(|res| ActivationStep {
            resource_id: res.id,
            action: plan_reopen(res),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_reopen_url() {
        let res = Resource::new(
            "Example",
            ResourceKind::Url,
            crate::models::ResourceIdentity {
                kind: ResourceKind::Url,
                descriptor: "https://example.com".to_string(),
                hint: None,
                reopen_info: Some("https://example.com".to_string()),
            },
        );

        match plan_reopen(&res) {
            ReopenAction::OpenUrl { url } => {
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("Expected OpenUrl"),
        }
    }

    // FR-9.7: a live tab has no URL — it must restore by re-focusing the tab,
    // never by opening a (fabricated) URL.
    #[test]
    fn test_plan_reopen_live_tab_is_focus_only() {
        let res = Resource::new(
            "GitHub - reiple/vibe-control",
            ResourceKind::BrowserTabLive,
            crate::models::ResourceIdentity {
                kind: ResourceKind::BrowserTabLive,
                descriptor: "GitHub - reiple/vibe-control".to_string(),
                hint: Some("chrome\u{1f}12345\u{1f}1".to_string()),
                reopen_info: None,
            },
        );

        assert!(matches!(
            plan_reopen(&res),
            ReopenAction::FocusLinkedWindow
        ));
    }

    #[test]
    fn test_plan_bundle_activation_order() {
        let mut bundle = WorkBundle::new("Test");
        for i in 0..3 {
            let res = Resource::new(
                format!("Resource {}", i),
                ResourceKind::Url,
                crate::models::ResourceIdentity {
                    kind: ResourceKind::Url,
                    descriptor: format!("https://example{}.com", i),
                    hint: None,
                    reopen_info: Some(format!("https://example{}.com", i)),
                },
            );
            bundle.add_resource(res);
        }

        let steps = plan_bundle_activation(&bundle);
        assert_eq!(steps.len(), 3);
        for (i, step) in steps.iter().enumerate() {
            assert_eq!(step.resource_id, bundle.resources[i].id);
        }
    }
}
