use std::fs;
use std::path::Path;

use crate::app::state::HookStatus;

/// Embedded hook script content (compiled into binary)
const HOOK_SCRIPT: &str = include_str!("../../hooks/send_event.sh");

/// Name of the hook script file
const HOOK_SCRIPT_NAME: &str = "send_event.sh";

/// Detect if loom-tui hook script is installed in the project.
///
/// Pure I/O boundary function: checks filesystem state but performs no mutations.
///
/// # Arguments
///
/// * `project_root` - Root directory of the project being monitored
///
/// # Returns
///
/// * `HookStatus::Installed` - If hook script exists and is executable
/// * `HookStatus::Missing` - If hook script does not exist
/// * `HookStatus::Unknown` - If unable to determine status (e.g., permission denied)
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use loom_tui::hook_install::detect_hook;
/// use loom_tui::app::state::HookStatus;
///
/// let project_root = Path::new("/home/user/project");
/// let status = detect_hook(project_root);
///
/// match status {
///     HookStatus::Installed => println!("Hooks installed"),
///     HookStatus::Missing => println!("Hooks not installed"),
///     HookStatus::Unknown => println!("Unable to check hook status"),
///     HookStatus::InstallFailed(_) => unreachable!("detect never returns InstallFailed"),
/// }
/// ```
pub fn detect_hook(project_root: &Path) -> HookStatus {
    let hook_path = project_root
        .join(".claude")
        .join("hooks")
        .join(HOOK_SCRIPT_NAME);

    match hook_path.exists() {
        true => {
            // File exists - check if it's executable (on Unix)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                match fs::metadata(&hook_path) {
                    Ok(metadata) => {
                        let permissions = metadata.permissions();
                        if permissions.mode() & 0o111 != 0 {
                            HookStatus::Installed
                        } else {
                            // Exists but not executable
                            HookStatus::Missing
                        }
                    }
                    Err(_) => HookStatus::Unknown,
                }
            }

            // On non-Unix, just check existence
            #[cfg(not(unix))]
            {
                HookStatus::Installed
            }
        }
        false => HookStatus::Missing,
    }
}

/// Install hook script to project's .claude/hooks/ directory.
///
/// I/O boundary function: creates directories and writes files.
///
/// # Arguments
///
/// * `project_root` - Root directory of the project
///
/// # Returns
///
/// * `Ok(())` - Hook successfully installed
/// * `Err(String)` - Installation failed with error message
///
/// # Behavior
///
/// 1. Creates `.claude/hooks/` directory if it doesn't exist
/// 2. Writes embedded hook script to `send_event.sh`
/// 3. Sets executable permission (Unix only)
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use loom_tui::hook_install::install_hook;
///
/// let project_root = Path::new("/home/user/project");
/// match install_hook(project_root) {
///     Ok(()) => println!("Hook installed successfully"),
///     Err(e) => eprintln!("Installation failed: {}", e),
/// }
/// ```
pub fn install_hook(project_root: &Path) -> Result<(), String> {
    let hooks_dir = project_root.join(".claude").join("hooks");
    let hook_path = hooks_dir.join(HOOK_SCRIPT_NAME);

    // Create hooks directory if missing
    fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("Failed to create hooks directory: {}", e))?;

    // Write hook script
    fs::write(&hook_path, HOOK_SCRIPT)
        .map_err(|e| format!("Failed to write hook script: {}", e))?;

    // Set executable permission (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&hook_path)
            .map_err(|e| format!("Failed to read hook metadata: {}", e))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)
            .map_err(|e| format!("Failed to set executable permission: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_hook_missing() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let status = detect_hook(project_root);
        assert!(matches!(status, HookStatus::Missing));
    }

    #[test]
    fn test_detect_hook_installed() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Install hook first
        install_hook(project_root).unwrap();

        let status = detect_hook(project_root);
        assert!(matches!(status, HookStatus::Installed));
    }

    #[test]
    fn test_detect_hook_exists_but_not_executable() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create hook directory
        let hooks_dir = project_root.join(".claude").join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Write hook without executable permission
        let hook_path = hooks_dir.join(HOOK_SCRIPT_NAME);
        fs::write(&hook_path, "#!/bin/sh\necho test\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&hook_path).unwrap().permissions();
            permissions.set_mode(0o644); // Not executable
            fs::set_permissions(&hook_path, permissions).unwrap();

            let status = detect_hook(project_root);
            assert!(matches!(status, HookStatus::Missing));
        }

        #[cfg(not(unix))]
        {
            // On Windows, existence is enough
            let status = detect_hook(project_root);
            assert!(matches!(status, HookStatus::Installed));
        }
    }

    #[test]
    fn test_install_hook_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // .claude/hooks/ should not exist yet
        let hooks_dir = project_root.join(".claude").join("hooks");
        assert!(!hooks_dir.exists());

        // Install hook
        let result = install_hook(project_root);
        assert!(result.is_ok());

        // Directory should now exist
        assert!(hooks_dir.exists());
        assert!(hooks_dir.is_dir());
    }

    #[test]
    fn test_install_hook_writes_script() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Install hook
        install_hook(project_root).unwrap();

        // Hook script should exist
        let hook_path = project_root
            .join(".claude")
            .join("hooks")
            .join(HOOK_SCRIPT_NAME);
        assert!(hook_path.exists());

        // Content should match embedded script
        let content = fs::read_to_string(&hook_path).unwrap();
        assert_eq!(content, HOOK_SCRIPT);
    }

    #[test]
    #[cfg(unix)]
    fn test_install_hook_sets_executable_permission() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Install hook
        install_hook(project_root).unwrap();

        // Check permissions
        let hook_path = project_root
            .join(".claude")
            .join("hooks")
            .join(HOOK_SCRIPT_NAME);
        let metadata = fs::metadata(&hook_path).unwrap();
        let permissions = metadata.permissions();

        // Should be executable (0o755)
        assert_eq!(permissions.mode() & 0o777, 0o755);
    }

    #[test]
    fn test_install_hook_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Install twice
        install_hook(project_root).unwrap();
        let result = install_hook(project_root);

        // Second install should succeed (overwrites)
        assert!(result.is_ok());

        // Hook should still be properly installed
        let status = detect_hook(project_root);
        assert!(matches!(status, HookStatus::Installed));
    }

    #[test]
    fn test_embedded_script_is_posix_compliant() {
        // Verify embedded script starts with shebang
        assert!(HOOK_SCRIPT.starts_with("#!/bin/sh"));

        // Verify script contains expected logic markers
        assert!(HOOK_SCRIPT.contains("TMPDIR"));
        assert!(HOOK_SCRIPT.contains("loom-tui"));
        assert!(HOOK_SCRIPT.contains("events.jsonl"));
        assert!(HOOK_SCRIPT.contains("mkdir -p"));
        assert!(HOOK_SCRIPT.contains("exit 0"));
    }

    #[test]
    fn test_hook_script_name_constant() {
        assert_eq!(HOOK_SCRIPT_NAME, "send_event.sh");
    }

    #[test]
    fn test_install_hook_with_existing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Pre-create .claude/hooks/ directory
        let hooks_dir = project_root.join(".claude").join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Install should still succeed
        let result = install_hook(project_root);
        assert!(result.is_ok());

        let status = detect_hook(project_root);
        assert!(matches!(status, HookStatus::Installed));
    }

    #[test]
    fn test_detect_hook_with_invalid_path() {
        // Use a path that doesn't exist and can't be created
        let project_root = Path::new("/nonexistent/invalid/path");

        let status = detect_hook(project_root);
        // Should return Missing since path doesn't exist
        assert!(matches!(status, HookStatus::Missing));
    }

    #[test]
    fn test_install_hook_error_message_format() {
        // Try to install to a read-only location (this may be platform-specific)
        // We test that the error contains useful information
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create .claude directory but make it read-only
        let claude_dir = project_root.join(".claude");
        fs::create_dir(&claude_dir).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&claude_dir).unwrap().permissions();
            permissions.set_mode(0o444); // Read-only
            fs::set_permissions(&claude_dir, permissions).unwrap();

            let result = install_hook(project_root);
            assert!(result.is_err());

            let error = result.unwrap_err();
            // Error message should mention the failure reason
            assert!(
                error.contains("Failed to create hooks directory")
                    || error.contains("Failed to write hook script")
            );
        }
    }
}
