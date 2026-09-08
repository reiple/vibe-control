use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type BundleId = Uuid;
pub type ResourceId = Uuid;

/// 리소스 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    WindowRef,
    BrowserTab,
    Folder,
    AppLaunch,
    Url,
    CodingSession,
}

/// 리소스 상태
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ResourceStatus {
    Active,
    Inactive,
    PermissionRequired,
    #[default]
    Unknown,
}

/// 코딩 세션 답변 완료여부 (3-상태)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionCompletion {
    Waiting,
    NotWaiting,
    Unknown,
}

/// 리소스 식별 정보 (복합 식별)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceIdentity {
    pub kind: ResourceKind,
    /// 안정 서술자 (영속, 매칭 근거)
    pub descriptor: String,
    /// 비영속 힌트 (PID/HWND/tab_id 등)
    pub hint: Option<String>,
    /// 재실행 정보
    pub reopen_info: Option<String>,
}

/// 작업 묶음 내 리소스
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub id: ResourceId,
    /// 표시 이름 (편집 가능)
    pub display_name: String,
    pub kind: ResourceKind,
    pub identity: ResourceIdentity,
    /// 캐시된 상태 (권위는 평가 시점)
    #[serde(skip)]
    pub status: ResourceStatus,
    /// 묶음 내 정렬 순서
    pub order: u32,
}

/// 작업 묶음
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkBundle {
    pub id: BundleId,
    pub name: String,
    /// 정렬 순서 보존
    pub resources: Vec<Resource>,
}

impl WorkBundle {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            resources: Vec::new(),
        }
    }

    /// 리소스 추가 (중복 검사 없음 — IdentityMatcher 책임)
    pub fn add_resource(&mut self, mut resource: Resource) {
        resource.order = self.resources.len() as u32;
        self.resources.push(resource);
    }

    /// 리소스 제거
    pub fn remove_resource(&mut self, id: ResourceId) {
        self.resources.retain(|r| r.id != id);
        self.reorder();
    }

    /// 정렬 순서 갱신
    fn reorder(&mut self) {
        for (idx, resource) in self.resources.iter_mut().enumerate() {
            resource.order = idx as u32;
        }
    }
}

impl Resource {
    pub fn new(
        display_name: impl Into<String>,
        kind: ResourceKind,
        identity: ResourceIdentity,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            display_name: display_name.into(),
            kind,
            identity,
            status: ResourceStatus::Unknown,
            order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_creation() {
        let bundle = WorkBundle::new("Test Bundle");
        assert_eq!(bundle.name, "Test Bundle");
        assert!(bundle.resources.is_empty());
    }

    #[test]
    fn test_bundle_add_remove() {
        let mut bundle = WorkBundle::new("Test");
        let resource = Resource::new(
            "Test Resource",
            ResourceKind::Folder,
            ResourceIdentity {
                kind: ResourceKind::Folder,
                descriptor: "/path/to/folder".to_string(),
                hint: None,
                reopen_info: Some("/path/to/folder".to_string()),
            },
        );
        let res_id = resource.id;
        bundle.add_resource(resource);
        assert_eq!(bundle.resources.len(), 1);
        bundle.remove_resource(res_id);
        assert!(bundle.resources.is_empty());
    }
}
