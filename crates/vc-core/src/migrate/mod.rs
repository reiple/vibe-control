use serde_json::{json, Value};

use crate::error::{CoreError, Result};
use crate::models::WorkBundle;

const CURRENT_VERSION: u32 = 1;

/// 현재 버전
pub fn current_version() -> u32 {
    CURRENT_VERSION
}

/// 버전 마이그레이션 (라운드트립 보장, SECURITY-13)
pub fn load_and_migrate(raw: &[u8]) -> Result<Vec<WorkBundle>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let value: Value = serde_json::from_slice(raw)
        .map_err(|e| CoreError::corrupt(format!("JSON parse error: {}", e)))?;

    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    if version > CURRENT_VERSION {
        return Err(CoreError::corrupt(
            "Unsupported future version (cannot read)",
        ));
    }

    // 버전별 마이그레이션 (현재 v1만)
    if version == 1 {
        let bundles: Vec<WorkBundle> = serde_json::from_value(value.get("bundles").cloned().unwrap_or(Value::Array(vec![])))
            .map_err(|e| CoreError::corrupt(format!("Bundles parse error: {}", e)))?;
        Ok(bundles)
    } else {
        Err(CoreError::corrupt("Unknown version"))
    }
}

/// 직렬화 (라운드트립 보장, PBT-02)
pub fn serialize(bundles: &[WorkBundle]) -> Result<Vec<u8>> {
    let value = json!({
        "version": CURRENT_VERSION,
        "bundles": bundles,
    });

    serde_json::to_vec(&value)
        .map_err(|e| CoreError::corrupt(format!("Serialization error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version() {
        assert_eq!(current_version(), 1);
    }

    #[test]
    fn test_serialize_empty() {
        let bundles: Vec<WorkBundle> = vec![];
        let serialized = serialize(&bundles).unwrap();
        assert!(!serialized.is_empty());
    }

    #[test]
    fn test_roundtrip() {
        let bundle = WorkBundle::new("Test Bundle");
        let bundles = vec![bundle.clone()];

        let serialized = serialize(&bundles).unwrap();
        let deserialized = load_and_migrate(&serialized).unwrap();

        assert_eq!(deserialized.len(), 1);
        assert_eq!(deserialized[0].name, bundle.name);
    }

    #[test]
    fn test_corrupt_data() {
        let corrupt = b"not valid json";
        assert!(load_and_migrate(corrupt).is_err());
    }
}

#[cfg(test)]
mod pbt {
    use super::*;
    use crate::models::{Resource, ResourceIdentity, ResourceKind};
    use proptest::prelude::*;

    fn resource_kind() -> impl Strategy<Value = ResourceKind> {
        prop_oneof![
            Just(ResourceKind::WindowRef),
            Just(ResourceKind::BrowserTab),
            Just(ResourceKind::Folder),
            Just(ResourceKind::AppLaunch),
            Just(ResourceKind::Url),
            Just(ResourceKind::CodingSession),
        ]
    }

    fn bundles_strategy() -> impl Strategy<Value = Vec<WorkBundle>> {
        let resource = (".*", resource_kind(), ".*", any::<Option<String>>()).prop_map(
            |(display_name, kind, descriptor, reopen_info)| {
                Resource::new(
                    display_name,
                    kind,
                    ResourceIdentity {
                        kind,
                        descriptor,
                        hint: None,
                        reopen_info,
                    },
                )
            },
        );
        let bundle = (".*", prop::collection::vec(resource, 0..5)).prop_map(|(name, resources)| {
            let mut b = WorkBundle::new(name);
            for r in resources {
                b.add_resource(r);
            }
            b
        });
        prop::collection::vec(bundle, 0..5)
    }

    proptest! {
        // PBT-02: serialize -> load -> serialize is stable (round-trip preserves data)
        #[test]
        fn prop_roundtrip_stable(bundles in bundles_strategy()) {
            let first = serialize(&bundles).unwrap();
            let loaded = load_and_migrate(&first).unwrap();
            let second = serialize(&loaded).unwrap();
            prop_assert_eq!(first, second);
            prop_assert_eq!(bundles.len(), loaded.len());
        }

        // PBT-03: parser never panics on arbitrary bytes (returns Ok or Err)
        #[test]
        fn prop_parser_robust(raw in prop::collection::vec(any::<u8>(), 0..512)) {
            let _ = load_and_migrate(&raw);
        }
    }
}
