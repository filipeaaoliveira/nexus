use std::fmt;

/// Error returned when a `ProfileId` cannot be constructed from invalid input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileIdError {
    /// The input string was empty or contained only whitespace.
    Empty,
}

impl fmt::Display for ProfileIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("profile id must not be empty"),
        }
    }
}

impl std::error::Error for ProfileIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProfileIdError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ProfileIdError::Empty);
        }
        Ok(Self(id))
    }

    /// Construct without validation. Use only when the id is known-valid
    /// (e.g. loaded from storage that validated at write time).
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ProfileId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ProfileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for ProfileId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ProfileId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ProfileId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ProfileId {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_string() {
        assert_eq!(ProfileId::new(""), Err(ProfileIdError::Empty));
    }

    #[test]
    fn new_rejects_whitespace_only() {
        assert_eq!(ProfileId::new("   "), Err(ProfileIdError::Empty));
    }

    #[test]
    fn new_accepts_valid_id() {
        assert!(ProfileId::new("profile-01").is_ok());
    }

    #[test]
    fn new_unchecked_accepts_anything() {
        let id = ProfileId::new_unchecked("");
        assert_eq!(id.as_str(), "");
    }

    #[test]
    fn display_shows_inner_string() {
        let id = ProfileId::new("test-id").unwrap();
        assert_eq!(id.to_string(), "test-id");
    }
}
