use std::fs;
use std::path::Path;

/// Detects which stored profile is currently "live" by comparing
/// the home auth file bytes against each profile's auth file.
pub fn detect_live_profile_id(
    profiles_dir: &Path,
    home_auth: &Path,
    auth_file_name: &str,
) -> Option<String> {
    let home_bytes = fs::read(home_auth).ok()?;

    let entries = fs::read_dir(profiles_dir).ok()?;
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();

    for id in ids {
        let profile_auth = profiles_dir.join(&id).join(auth_file_name);
        if let Ok(profile_bytes) = fs::read(&profile_auth) {
            if profile_bytes == home_bytes {
                return Some(id);
            }
        }
    }

    None
}
