use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::shared::process_core::tokio_command;

#[cfg(target_os = "linux")]
use serde::Deserialize;
#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use tokio::time::{sleep, Duration};

#[tauri::command]
pub(crate) async fn open_external_url(url: String) -> Result<(), String> {
    let url = sanitize_external_url(url)?;

    #[cfg(target_os = "linux")]
    {
        return open_external_url_linux(&url).await;
    }

    #[cfg(target_os = "macos")]
    {
        return open_external_url_macos(&url).await;
    }

    #[cfg(target_os = "windows")]
    {
        return open_external_url_windows(&url).await;
    }

    #[allow(unreachable_code)]
    Err("Opening external URLs is not supported on this platform.".to_string())
}

fn sanitize_external_url(url: String) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Missing URL.".to_string());
    }
    if trimmed.contains('\0') || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("URL contains unsupported control characters.".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(target_os = "macos")]
async fn open_external_url_macos(url: &str) -> Result<(), String> {
    let output = tokio_command("open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| format!("Failed to open URL with macOS open: {error}"))?;
    ensure_success("open", output)
}

#[cfg(target_os = "windows")]
async fn open_external_url_windows(url: &str) -> Result<(), String> {
    let output = tokio_command("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| format!("Failed to open URL with Windows start: {error}"))?;
    ensure_success("cmd /C start", output)
}

#[cfg(target_os = "linux")]
async fn open_external_url_linux(url: &str) -> Result<(), String> {
    let focus_candidates = resolve_linux_focus_candidates(url).await;

    let output = open_url_with_linux_handler(url).await?;
    ensure_success("xdg-open", output)?;

    if !focus_candidates.is_empty() {
        let _ = focus_linux_window(&focus_candidates).await;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
async fn open_url_with_linux_handler(url: &str) -> Result<std::process::Output, String> {
    let mut command = if command_exists("xdg-open") {
        tokio_command("xdg-open")
    } else if command_exists("gio") {
        let mut command = tokio_command("gio");
        command.arg("open");
        command
    } else {
        return Err("Neither `xdg-open` nor `gio open` is available.".to_string());
    };

    command
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| format!("Failed to open external URL on Linux: {error}"))
}

fn ensure_success(command_label: &str, output: std::process::Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let exit_detail = output
        .status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "terminated by signal".to_string());

    if stderr.is_empty() {
        Err(format!("{command_label} failed ({exit_detail})."))
    } else {
        Err(format!(
            "{command_label} failed ({exit_detail}; stderr: {stderr})."
        ))
    }
}

#[cfg(target_os = "linux")]
fn command_exists(program: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|dir| dir.join(program).is_file())
}

#[cfg(target_os = "linux")]
fn extract_url_scheme(url: &str) -> Option<&str> {
    url.split_once(':')
        .map(|(scheme, _)| scheme.trim())
        .filter(|scheme| !scheme.is_empty())
}

#[cfg(target_os = "linux")]
async fn resolve_linux_focus_candidates(url: &str) -> Vec<String> {
    let Some(scheme) = extract_url_scheme(url) else {
        return Vec::new();
    };

    let Some(desktop_entry) = query_linux_default_handler(scheme).await else {
        return Vec::new();
    };

    let mut candidates = derive_focus_candidates_from_desktop_id(&desktop_entry);
    if let Some(desktop_path) = find_desktop_entry_path(&desktop_entry) {
        if let Some(startup_wm_class) = read_desktop_entry_value(&desktop_path, "StartupWMClass") {
            candidates.push(startup_wm_class);
        }
        if let Some(exec) = read_desktop_entry_value(&desktop_path, "Exec") {
            if let Some(exec_program) = extract_exec_program(&exec) {
                candidates.push(exec_program);
            }
        }
    }

    dedupe_focus_candidates(candidates)
}

#[cfg(target_os = "linux")]
async fn query_linux_default_handler(scheme: &str) -> Option<String> {
    let query = format!("x-scheme-handler/{scheme}");
    let output = tokio_command("xdg-mime")
        .arg("query")
        .arg("default")
        .arg(query)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let desktop_entry = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if desktop_entry.is_empty() {
        None
    } else {
        Some(desktop_entry)
    }
}

#[cfg(target_os = "linux")]
fn derive_focus_candidates_from_desktop_id(desktop_entry: &str) -> Vec<String> {
    let trimmed = desktop_entry.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(trimmed);
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);

    let mut candidates = vec![stem.to_string()];
    if let Some(stripped) = stem.strip_suffix("-url-handler") {
        candidates.push(stripped.to_string());
    }
    candidates
}

#[cfg(target_os = "linux")]
fn dedupe_focus_candidates(candidates: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for candidate in candidates {
        let next = normalize_focus_candidate(&candidate);
        if next.is_empty() || normalized.contains(&next) {
            continue;
        }
        normalized.push(next);
    }
    normalized
}

#[cfg(target_os = "linux")]
fn normalize_focus_candidate(candidate: &str) -> String {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(trimmed);
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);

    stem.trim().to_ascii_lowercase()
}

#[cfg(target_os = "linux")]
fn desktop_entry_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(path).join("applications"));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    if let Some(paths) = env::var_os("XDG_DATA_DIRS") {
        dirs.extend(env::split_paths(&paths).map(|path| path.join("applications")));
    } else {
        dirs.push(PathBuf::from("/usr/local/share/applications"));
        dirs.push(PathBuf::from("/usr/share/applications"));
    }

    dirs
}

#[cfg(target_os = "linux")]
fn find_desktop_entry_path(desktop_entry: &str) -> Option<PathBuf> {
    desktop_entry_search_dirs()
        .into_iter()
        .map(|dir| dir.join(desktop_entry))
        .find(|path| path.is_file())
}

#[cfg(target_os = "linux")]
fn read_desktop_entry_value(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((candidate_key, value)) = line.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key {
            return Some(value.trim().to_string());
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn extract_exec_program(exec: &str) -> Option<String> {
    let tokens = shell_words::split(exec).ok()?;
    let mut iter = tokens.into_iter();
    let first = iter.next()?;

    if Path::new(&first)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("env"))
    {
        for token in iter {
            if token.contains('=') && !token.starts_with('/') {
                continue;
            }
            return Some(token);
        }
        return None;
    }

    Some(first)
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Deserialize)]
struct NiriWindowInfo {
    id: u64,
    app_id: Option<String>,
    title: Option<String>,
    focus_timestamp: Option<NiriFocusTimestamp>,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Deserialize)]
struct NiriFocusTimestamp {
    secs: u64,
    nanos: u32,
}

#[cfg(target_os = "linux")]
async fn focus_linux_window(candidates: &[String]) -> Result<(), String> {
    if candidates.is_empty() || !command_exists("niri") {
        return Ok(());
    }

    for _ in 0..12 {
        if focus_niri_window(candidates).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(150)).await;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
async fn focus_niri_window(candidates: &[String]) -> Result<bool, String> {
    let output = tokio_command("niri")
        .arg("msg")
        .arg("-j")
        .arg("windows")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map_err(|error| format!("Failed to query niri windows: {error}"))?;

    if !output.status.success() {
        return Ok(false);
    }

    let windows: Vec<NiriWindowInfo> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let target = windows
        .into_iter()
        .filter_map(|window| {
            let score = window_focus_match_score(&window, candidates)?;
            Some((score, window))
        })
        .max_by_key(|(score, window)| {
            let (secs, nanos) = window
                .focus_timestamp
                .as_ref()
                .map(|timestamp| (timestamp.secs, timestamp.nanos))
                .unwrap_or((0, 0));
            (*score, secs, nanos, window.id)
        });

    let Some((_, target)) = target else {
        return Ok(false);
    };

    let status = tokio_command("niri")
        .arg("msg")
        .arg("action")
        .arg("focus-window")
        .arg("--id")
        .arg(target.id.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|error| format!("Failed to focus niri window: {error}"))?;

    Ok(status.success())
}

#[cfg(target_os = "linux")]
fn window_focus_match_score(window: &NiriWindowInfo, candidates: &[String]) -> Option<u8> {
    let Some(app_id) = window.app_id.as_deref() else {
        return None;
    };
    let normalized_app_id = normalize_focus_candidate(app_id);
    if normalized_app_id.is_empty() {
        return None;
    }

    if candidates
        .iter()
        .any(|candidate| candidate == &normalized_app_id)
    {
        return Some(2);
    }

    let title = window.title.as_deref()?;
    if candidates
        .iter()
        .any(|candidate| title_matches_focus_candidate(title, candidate))
    {
        return Some(1);
    }

    None
}

#[cfg(target_os = "linux")]
fn title_matches_focus_candidate(title: &str, candidate: &str) -> bool {
    let title_tokens = normalize_focus_tokens(title);
    let candidate_tokens = normalize_focus_tokens(candidate);
    if title_tokens.is_empty() || candidate_tokens.is_empty() {
        return false;
    }

    candidate_tokens
        .iter()
        .all(|candidate_token| title_tokens.iter().any(|token| token == candidate_token))
}

#[cfg(target_os = "linux")]
fn normalize_focus_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        dedupe_focus_candidates, derive_focus_candidates_from_desktop_id, extract_exec_program,
        extract_url_scheme, normalize_focus_candidate, normalize_focus_tokens,
        title_matches_focus_candidate,
    };

    #[test]
    fn extracts_url_scheme() {
        assert_eq!(extract_url_scheme("https://example.com"), Some("https"));
        assert_eq!(
            extract_url_scheme("vscode://file/tmp/example.ts"),
            Some("vscode")
        );
        assert_eq!(extract_url_scheme(""), None);
    }

    #[test]
    fn derives_focus_candidates_from_desktop_url_handlers() {
        assert_eq!(
            derive_focus_candidates_from_desktop_id("code-url-handler.desktop"),
            vec!["code-url-handler".to_string(), "code".to_string()]
        );
        assert_eq!(
            derive_focus_candidates_from_desktop_id("firefox.desktop"),
            vec!["firefox".to_string()]
        );
    }

    #[test]
    fn extracts_exec_program_from_desktop_entry_exec() {
        assert_eq!(
            extract_exec_program("/usr/lib/firefox/firefox %u"),
            Some("/usr/lib/firefox/firefox".to_string())
        );
        assert_eq!(
            extract_exec_program("/usr/bin/code --open-url %U"),
            Some("/usr/bin/code".to_string())
        );
        assert_eq!(
            extract_exec_program("env BAMF_DESKTOP_FILE_HINT=/usr/share/applications/code.desktop /usr/bin/code --open-url %U"),
            Some("/usr/bin/code".to_string())
        );
    }

    #[test]
    fn normalizes_focus_candidates_from_paths() {
        assert_eq!(normalize_focus_candidate("/usr/bin/code"), "code");
        assert_eq!(normalize_focus_candidate("Firefox"), "firefox");
        assert_eq!(
            normalize_focus_candidate("code-url-handler.desktop"),
            "code-url-handler"
        );
    }

    #[test]
    fn dedupes_focus_candidates_after_normalization() {
        assert_eq!(
            dedupe_focus_candidates(vec![
                "Firefox".to_string(),
                "/usr/lib/firefox/firefox".to_string(),
                "firefox.desktop".to_string(),
            ]),
            vec!["firefox".to_string()]
        );
    }

    #[test]
    fn tokenizes_focus_titles_and_candidates() {
        assert_eq!(
            normalize_focus_tokens("Example Domain — Mozilla Firefox"),
            vec!["example", "domain", "mozilla", "firefox"]
        );
        assert_eq!(
            normalize_focus_tokens("microsoft-edge.desktop"),
            vec!["microsoft", "edge", "desktop"]
        );
    }

    #[test]
    fn matches_titles_by_tokens_without_false_positive_substrings() {
        assert!(title_matches_focus_candidate(
            "Example Domain — Mozilla Firefox",
            "firefox"
        ));
        assert!(title_matches_focus_candidate(
            "WncFht - Microsoft Edge",
            "microsoft-edge"
        ));
        assert!(!title_matches_focus_candidate("Codex Monitor", "code"));
    }
}
