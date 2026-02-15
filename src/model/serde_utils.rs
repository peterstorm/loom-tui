/// Shared serde utilities for domain models
/// Custom serde for Duration as milliseconds
pub mod duration_opt_millis {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => serializer.serialize_some(&d.as_millis()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis: Option<u64> = Option::deserialize(deserializer)?;
        Ok(millis.map(Duration::from_millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct TestStruct {
        #[serde(
            default,
            with = "duration_opt_millis",
            skip_serializing_if = "Option::is_none"
        )]
        duration: Option<Duration>,
    }

    #[test]
    fn duration_serializes_as_millis() {
        let obj = TestStruct {
            duration: Some(Duration::from_millis(250)),
        };

        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("\"duration\":250"));
    }

    #[test]
    fn duration_deserializes_from_millis() {
        let json = r#"{"duration":250}"#;
        let obj: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(obj.duration, Some(Duration::from_millis(250)));
    }

    #[test]
    fn none_duration_omitted_from_json() {
        let obj = TestStruct { duration: None };
        let json = serde_json::to_string(&obj).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn round_trip_preserves_duration() {
        let original = TestStruct {
            duration: Some(Duration::from_millis(1500)),
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: TestStruct = serde_json::from_str(&json).unwrap();

        assert_eq!(original.duration, restored.duration);
    }
}
