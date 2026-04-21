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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn match_returns_profile_id() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join("auth.json"), b"{\"email\":\"a@b.com\"}").unwrap();

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"a@b.com\"}").unwrap();

        assert_eq!(
            detect_live_profile_id(&profiles_dir, &home_auth, "auth.json"),
            Some("profile-01".to_string())
        );
    }

    #[test]
    fn no_match_returns_none() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join("auth.json"), b"{\"email\":\"a@b.com\"}").unwrap();

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"other@b.com\"}").unwrap();

        assert_eq!(
            detect_live_profile_id(&profiles_dir, &home_auth, "auth.json"),
            None
        );
    }

    #[test]
    fn missing_home_auth_returns_none() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join("auth.json"), b"{\"email\":\"a@b.com\"}").unwrap();

        let home_auth = tmp.path().join("nonexistent-auth.json");
        assert_eq!(
            detect_live_profile_id(&profiles_dir, &home_auth, "auth.json"),
            None
        );
    }

    #[test]
    fn empty_profiles_dir_returns_none() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"a@b.com\"}").unwrap();

        assert_eq!(
            detect_live_profile_id(&profiles_dir, &home_auth, "auth.json"),
            None
        );
    }
}
