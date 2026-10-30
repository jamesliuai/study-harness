use crate::contracts::CredentialStatus;
use crate::db::Result;
use zeroize::Zeroizing;
pub trait SecretStore: Send + Sync {
    fn read(&self) -> Result<Option<Zeroizing<String>>>;
    fn write(&self, key: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}
pub struct NativeKeychain {
    account: String,
}
impl NativeKeychain {
    pub fn initialize() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let store = apple_native_keyring_store::keychain::Store::new()
                .map_err(|_| "Apple Keychain is unavailable. Choose session-only storage.")?;
            keyring_core::set_default_store(store);
            Ok(Self {
                account: "openai".into(),
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(
                "Native storage is not configured for this platform. Choose session-only storage."
                    .into(),
            )
        }
    }
    fn entry(&self) -> Result<keyring_core::Entry> {
        keyring_core::Entry::new("dev.studyharness.desktop", &self.account)
            .map_err(|_| "Native credential store is unavailable.".into())
    }
}
impl SecretStore for NativeKeychain {
    fn read(&self) -> Result<Option<Zeroizing<String>>> {
        match self.entry()?.get_password() {
            Ok(s) => Ok(Some(Zeroizing::new(s))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => {
                Err("Keychain access failed. Unlock it or choose session-only storage.".into())
            }
        }
    }
    fn write(&self, key: &str) -> Result<()> {
        self.entry()?.set_password(key).map_err(|_| {
            "Could not save in Keychain. Choose session-only storage explicitly.".into()
        })
    }
    fn delete(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err("Could not remove the stored Keychain credential.".into()),
        }
    }
}
pub struct Credentials {
    native: Option<Box<dyn SecretStore>>,
    session: Option<Zeroizing<String>>,
}
impl Credentials {
    pub fn without_native() -> Self {
        Self {
            native: None,
            session: None,
        }
    }
    pub fn new() -> Self {
        Self {
            native: NativeKeychain::initialize()
                .ok()
                .map(|x| Box::new(x) as Box<dyn SecretStore>),
            session: None,
        }
    }
    pub fn status(&self) -> CredentialStatus {
        let (configured, storage, message) = if self.session.is_some() {
            (
                true,
                "session only",
                "Key held in Rust memory until the app exits.",
            )
        } else {
            match self.native.as_ref().map(|n| n.read()) {
                Some(Ok(Some(_))) => (true, "Apple Keychain", "Saved in macOS Keychain."),
                Some(Ok(None)) => (
                    false,
                    "not configured",
                    "Enter your OpenAI API key in Settings.",
                ),
                _ => (
                    false,
                    "unavailable",
                    "Native store unavailable. Session-only storage remains available.",
                ),
            }
        };
        CredentialStatus {
            configured,
            storage: storage.into(),
            masked: if configured {
                "••••••••"
            } else {
                "Not configured"
            }
            .into(),
            message: message.into(),
        }
    }
    pub fn set(&mut self, key: String, persist: bool) -> Result<CredentialStatus> {
        let key = Zeroizing::new(key);
        if key.len() < 16
            || key.len() > 1024
            || !key.is_ascii()
            || key.chars().any(char::is_whitespace)
        {
            return Err("Enter a valid API key without whitespace.".into());
        }
        if persist {
            self.native
                .as_ref()
                .ok_or("Keychain unavailable. Select session-only storage.")?
                .write(&key)?;
            self.session = None;
        } else {
            self.session = Some(key);
        }
        Ok(self.status())
    }
    pub fn key(&self) -> Result<Zeroizing<String>> {
        if let Some(s) = &self.session {
            return Ok(Zeroizing::new(s.to_string()));
        }
        self.native
            .as_ref()
            .ok_or("Add an API key in Settings.")?
            .read()?
            .ok_or("Add an API key in Settings.".into())
    }
    pub fn remove(&mut self) -> Result<CredentialStatus> {
        self.session = None;
        if let Some(n) = &self.native {
            n.delete()?;
        }
        Ok(self.status())
    }
}
/// Isolated native self-test; never touches the OpenAI account or prints a secret.
#[cfg(all(debug_assertions, not(feature = "offline-provider")))]
pub fn native_smoke() -> Result<serde_json::Value> {
    let mut backend = NativeKeychain::initialize()?;
    backend.account = format!("disposable-smoke-{}", uuid::Uuid::new_v4());
    let account = backend.account.clone();
    let sentinel = format!("not-an-api-key-{}", uuid::Uuid::new_v4());
    let mut c = Credentials {
        native: Some(Box::new(backend)),
        session: None,
    };
    let saved = c.set(sentinel.clone(), true)?;
    if !saved.configured
        || saved.storage != "Apple Keychain"
        || serde_json::to_string(&saved).unwrap().contains(&sentinel)
    {
        return Err("Native masked status check failed.".into());
    }
    drop(c);
    let mut backend = NativeKeychain::initialize()?;
    backend.account = account;
    let mut reopened = Credentials {
        native: Some(Box::new(backend)),
        session: None,
    };
    if reopened.key()?.as_str() != sentinel {
        return Err("Native reopen check failed.".into());
    }
    reopened.remove()?;
    if reopened.status().configured {
        return Err("Native deletion check failed.".into());
    }
    let mut fallback = Credentials {
        native: None,
        session: None,
    };
    fallback.set(sentinel.clone(), false)?;
    if fallback.status().storage != "session only" {
        return Err("Session fallback check failed.".into());
    }
    drop(fallback);
    let empty = Credentials {
        native: None,
        session: None,
    };
    if empty.key().is_ok() {
        return Err("Session secret survived its owner.".into());
    }
    Ok(
        serde_json::json!({"native_store":"Apple Keychain", "write_mask_reopen_delete":"pass", "session_only_lifetime":"pass", "secret_returned":false, "production_account_touched":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_masked_never_serializes_secret() {
        let mut c = Credentials {
            native: None,
            session: None,
        };
        let key = "fixture-test-secret-never-a-live-key";
        assert!(c.set(key.into(), true).is_err());
        let status = c.set(key.into(), false).unwrap();
        assert_eq!(status.storage, "session only");
        assert!(!serde_json::to_string(&status).unwrap().contains(key));
        assert_eq!(c.key().unwrap().as_str(), key);
        assert!(!c.remove().unwrap().configured);
    }
}
