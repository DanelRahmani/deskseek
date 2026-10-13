//! API keys live in the OS keychain (Credential Manager / Keychain / Secret
//! Service) — never in a config file and never in a log.
//!
//! One account per endpoint. An endpoint's `key_ref` is the account name, and a
//! blank `key_ref` means the endpoint needs no key at all, which is the honest
//! setting for a model on localhost.

const SERVICE: &str = "deskseek";

/// The account the app's key has always been stored under.
///
/// Endpoint one keeps it, so an existing install does not paste its key again —
/// which is the one visible cost this whole feature could have had.
pub const DEFAULT_REF: &str = "api_key";

fn entry(key_ref: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, key_ref).map_err(|e| e.to_string())
}

pub fn set_key(key_ref: &str, key: &str) -> Result<(), String> {
    if key_ref.trim().is_empty() {
        return Err("this endpoint has no key reference".to_string());
    }
    entry(key_ref)?
        .set_password(key)
        .map_err(|e| format!("could not store the key in the OS keychain: {e}"))
}

pub fn get_key(key_ref: &str) -> Result<Option<String>, String> {
    if key_ref.trim().is_empty() {
        return Ok(None);
    }
    match entry(key_ref)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("could not read the key from the OS keychain: {e}")),
    }
}

pub fn has_key(key_ref: &str) -> bool {
    matches!(get_key(key_ref), Ok(Some(key)) if !key.is_empty())
}

pub fn delete_key(key_ref: &str) -> Result<(), String> {
    if key_ref.trim().is_empty() {
        return Ok(());
    }
    match entry(key_ref)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not remove the key from the OS keychain: {e}")),
    }
}

/// A masked form for display, e.g. `sk-abc…1234`.
pub fn mask(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        return "•".repeat(n.max(4));
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_never_reveals_the_middle() {
        let m = mask("sk-abcdefghijklmnop");
        assert!(m.starts_with("sk-a"));
        assert!(m.ends_with("mnop"));
        assert!(!m.contains("efghijkl"));
    }

    #[test]
    fn short_keys_are_fully_masked() {
        assert_eq!(mask("abc"), "••••");
    }

    #[test]
    fn an_endpoint_with_no_key_reference_never_touches_the_keychain() {
        // A local model has no key, and asking the OS store about a blank account
        // is not just pointless — it can surface an error that is not one.
        assert_eq!(get_key("").unwrap(), None);
        assert!(!has_key("   "));
        assert!(delete_key("").is_ok());
        assert!(set_key("", "sk-abc").is_err());
    }

    #[test]
    fn the_default_reference_is_the_account_the_app_already_used() {
        // Renaming this would silently orphan the stored key, so it is asserted
        // rather than left to a comment.
        assert_eq!(DEFAULT_REF, "api_key");
        assert_eq!(crate::api::provider::seed()[0].key_ref, DEFAULT_REF);
    }
}
