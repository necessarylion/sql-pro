//! Saved passwords. They live in the OS keychain, never in a file.

use keyring::Entry;

use crate::constants::KEYCHAIN_SERVICE;
use crate::error::AppResult;

pub struct PasswordRepository;

impl PasswordRepository {
    pub fn save(&self, id: &str, password: &str) -> AppResult<()> {
        Ok(Entry::new(KEYCHAIN_SERVICE, id)?.set_password(password)?)
    }

    /// Gives an empty string when no password is saved.
    pub fn load(&self, id: &str) -> AppResult<String> {
        match Entry::new(KEYCHAIN_SERVICE, id)?.get_password() {
            Ok(password) => Ok(password),
            Err(keyring::Error::NoEntry) => Ok(String::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        match Entry::new(KEYCHAIN_SERVICE, id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
