use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                let id = s.into();
                assert!(!id.is_empty(), "{} cannot be empty", stringify!($name));
                Self(id)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self::new(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::new(s)
            }
        }
    };
}

id_newtype!(AgentId);
id_newtype!(SessionId);
id_newtype!(TaskId);
id_newtype!(ToolName);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "AgentId cannot be empty")]
    fn agent_id_empty_string_panics() {
        AgentId::new("");
    }

    #[test]
    #[should_panic(expected = "SessionId cannot be empty")]
    fn session_id_empty_string_panics() {
        SessionId::new("");
    }

    #[test]
    #[should_panic(expected = "TaskId cannot be empty")]
    fn task_id_empty_string_panics() {
        TaskId::new("");
    }

    #[test]
    #[should_panic(expected = "ToolName cannot be empty")]
    fn tool_name_empty_string_panics() {
        ToolName::new("");
    }

    #[test]
    fn agent_id_valid_non_empty() {
        let id = AgentId::new("a01");
        assert_eq!(id.as_str(), "a01");
    }

    #[test]
    fn task_id_from_str() {
        let id: TaskId = "T1".into();
        assert_eq!(id.as_str(), "T1");
    }

    #[test]
    fn task_id_from_string() {
        let id: TaskId = String::from("T2").into();
        assert_eq!(id.as_str(), "T2");
    }
}
