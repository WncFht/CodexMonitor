use std::path::Path;
use std::process::Stdio;

use crate::shared::process_core::tokio_command;

#[cfg(target_os = "linux")]
use serde::Deserialize;
#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use tokio::time::{sleep, Duration};

#[cfg(target_os = "linux")]
pub(crate) fn command_exists(program: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|dir| dir.join(program).is_file())
}

#[cfg(target_os = "linux")]
pub(crate) fn dedupe_focus_candidates(candidates: Vec<String>) -> Vec<String> {
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
pub(crate) fn normalize_focus_candidate(candidate: &str) -> String {
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
pub(crate) fn normalize_focus_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(target_os = "linux")]
pub(crate) async fn focus_linux_window(candidates: &[String]) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::{
        dedupe_focus_candidates, normalize_focus_candidate, normalize_focus_tokens,
        title_matches_focus_candidate,
    };

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
