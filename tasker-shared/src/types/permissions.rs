//! # Permission Vocabulary
//!
//! Compile-time permission vocabulary for API-level security.
//! Permissions follow the `resource:action` pattern with wildcard support.

use std::fmt;

/// All known permissions in the system.
///
/// Each variant maps to a `resource:action` string representation.
/// Permission checks support both exact matching and resource-level wildcards
/// (e.g., `tasks:*` matches all task permissions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // Task permissions
    TasksCreate,
    TasksRead,
    TasksList,
    TasksCancel,
    TasksContextRead,

    // Step permissions
    StepsRead,
    StepsResolve,

    // DLQ permissions
    DlqRead,
    DlqUpdate,
    DlqStats,

    // Template permissions
    TemplatesRead,
    TemplatesValidate,

    // System permissions
    SystemConfigRead,
    HandlersRead,
    AnalyticsRead,

    // Worker-specific permissions
    WorkerConfigRead,
    WorkerTemplatesRead,
}

impl Permission {
    /// String representation in `resource:action` format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TasksCreate => "tasks:create",
            Self::TasksRead => "tasks:read",
            Self::TasksList => "tasks:list",
            Self::TasksCancel => "tasks:cancel",
            Self::TasksContextRead => "tasks:context_read",
            Self::StepsRead => "steps:read",
            Self::StepsResolve => "steps:resolve",
            Self::DlqRead => "dlq:read",
            Self::DlqUpdate => "dlq:update",
            Self::DlqStats => "dlq:stats",
            Self::TemplatesRead => "templates:read",
            Self::TemplatesValidate => "templates:validate",
            Self::SystemConfigRead => "system:config_read",
            Self::HandlersRead => "system:handlers_read",
            Self::AnalyticsRead => "system:analytics_read",
            Self::WorkerConfigRead => "worker:config_read",
            Self::WorkerTemplatesRead => "worker:templates_read",
        }
    }

    /// Resource component of the permission (before the colon).
    pub fn resource(&self) -> &'static str {
        match self {
            Self::TasksCreate
            | Self::TasksRead
            | Self::TasksList
            | Self::TasksCancel
            | Self::TasksContextRead => "tasks",
            Self::StepsRead | Self::StepsResolve => "steps",
            Self::DlqRead | Self::DlqUpdate | Self::DlqStats => "dlq",
            Self::TemplatesRead | Self::TemplatesValidate => "templates",
            Self::SystemConfigRead | Self::HandlersRead | Self::AnalyticsRead => "system",
            Self::WorkerConfigRead | Self::WorkerTemplatesRead => "worker",
        }
    }

    /// Attempt to parse a permission string into a `Permission` variant.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "tasks:create" => Some(Self::TasksCreate),
            "tasks:read" => Some(Self::TasksRead),
            "tasks:list" => Some(Self::TasksList),
            "tasks:cancel" => Some(Self::TasksCancel),
            "tasks:context_read" => Some(Self::TasksContextRead),
            "steps:read" => Some(Self::StepsRead),
            "steps:resolve" => Some(Self::StepsResolve),
            "dlq:read" => Some(Self::DlqRead),
            "dlq:update" => Some(Self::DlqUpdate),
            "dlq:stats" => Some(Self::DlqStats),
            "templates:read" => Some(Self::TemplatesRead),
            "templates:validate" => Some(Self::TemplatesValidate),
            "system:config_read" => Some(Self::SystemConfigRead),
            "system:handlers_read" => Some(Self::HandlersRead),
            "system:analytics_read" => Some(Self::AnalyticsRead),
            "worker:config_read" => Some(Self::WorkerConfigRead),
            "worker:templates_read" => Some(Self::WorkerTemplatesRead),
            _ => None,
        }
    }

    /// All known permissions in the system.
    pub fn all() -> &'static [Permission] {
        &[
            Self::TasksCreate,
            Self::TasksRead,
            Self::TasksList,
            Self::TasksCancel,
            Self::TasksContextRead,
            Self::StepsRead,
            Self::StepsResolve,
            Self::DlqRead,
            Self::DlqUpdate,
            Self::DlqStats,
            Self::TemplatesRead,
            Self::TemplatesValidate,
            Self::SystemConfigRead,
            Self::HandlersRead,
            Self::AnalyticsRead,
            Self::WorkerConfigRead,
            Self::WorkerTemplatesRead,
        ]
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Check if a single claimed permission string matches a required permission.
///
/// Supports:
/// - `*` (global wildcard, matches everything)
/// - `resource:*` (resource wildcard, e.g. `tasks:*` matches `tasks:create`)
/// - Exact match (e.g. `tasks:create` matches `tasks:create`)
pub fn permission_matches(claimed: &str, required: &Permission) -> bool {
    if claimed == "*" {
        return true;
    }

    let required_str = required.as_str();

    // Exact match
    if claimed == required_str {
        return true;
    }

    // Resource wildcard: "resource:*"
    if let Some(prefix) = claimed.strip_suffix(":*") {
        return required.resource() == prefix;
    }

    false
}

/// Check if a set of claimed permissions includes the required permission.
pub fn has_permission(claimed: &[String], required: &Permission) -> bool {
    claimed.iter().any(|c| permission_matches(c, required))
}

/// Validate a list of permission strings and return any that are not recognized.
///
/// Resource wildcards (e.g., `tasks:*`) and the global wildcard (`*`) are considered valid.
pub fn validate_permissions(claimed: &[String]) -> Vec<String> {
    claimed
        .iter()
        .filter(|p| {
            let s = p.as_str();
            if s == "*" {
                return false; // valid
            }
            if let Some(prefix) = s.strip_suffix(":*") {
                // Check if the resource prefix is known
                let known_resources = ["tasks", "steps", "dlq", "templates", "system", "worker"];
                return !known_resources.contains(&prefix);
            }
            Permission::from_str_opt(s).is_none()
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_as_str_roundtrip() {
        for perm in Permission::all() {
            let s = perm.as_str();
            assert_eq!(
                Permission::from_str_opt(s),
                Some(*perm),
                "Roundtrip failed for {s}"
            );
        }
    }

    #[test]
    fn test_permission_resource() {
        assert_eq!(Permission::TasksCreate.resource(), "tasks");
        assert_eq!(Permission::StepsRead.resource(), "steps");
        assert_eq!(Permission::DlqStats.resource(), "dlq");
        assert_eq!(Permission::TemplatesRead.resource(), "templates");
        assert_eq!(Permission::SystemConfigRead.resource(), "system");
        assert_eq!(Permission::WorkerConfigRead.resource(), "worker");
    }

    #[test]
    fn test_permission_matches_exact() {
        assert!(permission_matches("tasks:create", &Permission::TasksCreate));
        assert!(!permission_matches("tasks:read", &Permission::TasksCreate));
    }

    #[test]
    fn test_permission_matches_global_wildcard() {
        assert!(permission_matches("*", &Permission::TasksCreate));
        assert!(permission_matches("*", &Permission::DlqStats));
        assert!(permission_matches("*", &Permission::WorkerConfigRead));
    }

    #[test]
    fn test_permission_matches_resource_wildcard() {
        assert!(permission_matches("tasks:*", &Permission::TasksCreate));
        assert!(permission_matches("tasks:*", &Permission::TasksRead));
        assert!(permission_matches("tasks:*", &Permission::TasksList));
        assert!(permission_matches("tasks:*", &Permission::TasksCancel));
        assert!(!permission_matches("tasks:*", &Permission::StepsRead));
        assert!(!permission_matches("tasks:*", &Permission::DlqRead));

        assert!(permission_matches("system:*", &Permission::SystemConfigRead));
        assert!(permission_matches("system:*", &Permission::HandlersRead));
        assert!(permission_matches("system:*", &Permission::AnalyticsRead));
        assert!(!permission_matches("system:*", &Permission::TasksCreate));
    }

    #[test]
    fn test_has_permission() {
        let claimed = vec![
            "tasks:create".to_string(),
            "tasks:read".to_string(),
            "dlq:*".to_string(),
        ];

        assert!(has_permission(&claimed, &Permission::TasksCreate));
        assert!(has_permission(&claimed, &Permission::TasksRead));
        assert!(!has_permission(&claimed, &Permission::TasksList));
        assert!(has_permission(&claimed, &Permission::DlqRead));
        assert!(has_permission(&claimed, &Permission::DlqUpdate));
        assert!(has_permission(&claimed, &Permission::DlqStats));
    }

    #[test]
    fn test_has_permission_global_wildcard() {
        let claimed = vec!["*".to_string()];
        for perm in Permission::all() {
            assert!(has_permission(&claimed, perm));
        }
    }

    #[test]
    fn test_has_permission_empty_claims() {
        let claimed: Vec<String> = vec![];
        assert!(!has_permission(&claimed, &Permission::TasksCreate));
    }

    #[test]
    fn test_validate_permissions_all_valid() {
        let perms = vec![
            "tasks:create".to_string(),
            "steps:read".to_string(),
            "*".to_string(),
            "dlq:*".to_string(),
        ];
        assert!(validate_permissions(&perms).is_empty());
    }

    #[test]
    fn test_validate_permissions_unknown() {
        let perms = vec![
            "tasks:create".to_string(),
            "unknown:action".to_string(),
            "tasks:delete".to_string(),
            "bogus:*".to_string(),
        ];
        let unknown = validate_permissions(&perms);
        assert_eq!(unknown.len(), 3);
        assert!(unknown.contains(&"unknown:action".to_string()));
        assert!(unknown.contains(&"tasks:delete".to_string()));
        assert!(unknown.contains(&"bogus:*".to_string()));
    }

    #[test]
    fn test_validate_permissions_known_wildcards() {
        let perms = vec![
            "tasks:*".to_string(),
            "steps:*".to_string(),
            "dlq:*".to_string(),
            "templates:*".to_string(),
            "system:*".to_string(),
            "worker:*".to_string(),
        ];
        assert!(validate_permissions(&perms).is_empty());
    }

    #[test]
    fn test_permission_display() {
        assert_eq!(format!("{}", Permission::TasksCreate), "tasks:create");
        assert_eq!(format!("{}", Permission::DlqStats), "dlq:stats");
    }

    #[test]
    fn test_from_str_opt_unknown() {
        assert_eq!(Permission::from_str_opt("unknown"), None);
        assert_eq!(Permission::from_str_opt("tasks:delete"), None);
        assert_eq!(Permission::from_str_opt(""), None);
    }
}
