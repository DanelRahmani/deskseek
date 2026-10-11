//! The API key lives in the OS keychain (Credential Manager / Keychain /
//! Secret Service) — never in a config file and never in a log.

const SERVICE: &str = "deskseek";
const ACCOUNT: &str = "api_key";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

pub fn set_api_key(key: &str) -> Result<(), String> {
    entry()?
        .set_password(key)
        .map_err(|e| format!("could not store key in the OS keychain: {e}"))
}

pub fn get_api_key() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("could not read key from the OS keychain: {e}")),
    }
}

pub fn has_api_key() -> bool {
    matches!(get_api_key(), Ok(Some(k)) if !k.is_empty())
}

pub fn delete_api_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not remove key from the OS keychain: {e}")),
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
}
