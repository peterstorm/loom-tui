/// Shared serde utilities for domain models

/// Deserialize a Vec<T> from JSON with element-wise fallback.
///
/// Each array element is deserialized independently: valid elements are kept,
/// invalid ones are silently skipped. This preserves good events when a v2
/// archive contains one corrupted entry, while still returning an empty vec
/// for old-format archives where no element matches T (FR-026, SC-008).
pub fn deserialize_vec_or_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let raw: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    if let serde_json::Value::Array(arr) = raw {
        return Ok(arr
            .into_iter()
            .filter_map(|v| serde_json::from_value::<T>(v).ok())
            .collect());
    }
    Ok(Vec::new())
}

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

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct IntWrapper {
        #[serde(default, deserialize_with = "deserialize_vec_or_empty")]
        values: Vec<u32>,
    }

    /// Valid elements survive; a single bad element is skipped, not the whole array.
    #[test]
    fn mixed_valid_invalid_elements_preserves_valid() {
        // 42 and 99 are valid u32; "bad" is not.
        let json = r#"{"values": [42, "bad", 99]}"#;
        let result: IntWrapper = serde_json::from_str(json).unwrap();
        assert_eq!(result.values, vec![42u32, 99u32]);
    }

    /// All-invalid array yields empty vec.
    #[test]
    fn all_invalid_elements_yields_empty_vec() {
        let json = r#"{"values": ["a", "b", "c"]}"#;
        let result: IntWrapper = serde_json::from_str(json).unwrap();
        assert!(result.values.is_empty());
    }

    /// Non-array value yields empty vec.
    #[test]
    fn non_array_value_yields_empty_vec() {
        let json = r#"{"values": "not-an-array"}"#;
        let result: IntWrapper = serde_json::from_str(json).unwrap();
        assert!(result.values.is_empty());
    }

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
