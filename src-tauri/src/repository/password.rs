//! Saved passwords. They live in the OS keychain, never in a file.
//!
//! All passwords are in ONE keychain item, the vault. It holds a JSON object
//! `{ id: password }`. macOS asks for permission per item. With one item it
//! asks one time, not one time for each saved connection.
//!
//! The vault is read one time and then kept in memory until the app closes.
//!
//! Older versions kept one item per password. `load` moves such an item into
//! the vault the first time it is asked for.

use std::collections::HashMap;
use std::sync::Arc;

use keyring::Entry;
use parking_lot::Mutex;

use crate::constants::{KEYCHAIN_SERVICE, KEYCHAIN_VAULT_ACCOUNT};
use crate::error::{AppError, AppResult};

/// The error for a vault item that does not hold the JSON we wrote.
/// It never shows what is in the item.
const VAULT_BROKEN: &str = "The saved passwords in the keychain cannot be read";

/// The error for an id that is the name of the vault item itself.
const ID_RESERVED: &str = "This id cannot be used for a password";

type Vault = HashMap<String, String>;

/// One secret per account name. The real one is the OS keychain.
pub trait Keychain: Send + Sync {
    /// Gives `None` when the account has no secret.
    fn get(&self, account: &str) -> AppResult<Option<String>>;

    fn set(&self, account: &str, secret: &str) -> AppResult<()>;

    /// It is not an error when the account has no secret.
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// The keychain of the operating system.
pub struct OsKeychain;

impl Keychain for OsKeychain {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        match Entry::new(KEYCHAIN_SERVICE, account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        Ok(Entry::new(KEYCHAIN_SERVICE, account)?.set_password(secret)?)
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match Entry::new(KEYCHAIN_SERVICE, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

pub struct PasswordRepository {
    keychain: Arc<dyn Keychain>,
    /// `None` until the vault item was read with success.
    /// The lock is held while we talk to the keychain. So two loads at the
    /// same time give one permission prompt, not two.
    vault: Mutex<Option<Vault>>,
}

impl PasswordRepository {
    pub fn new(keychain: Arc<dyn Keychain>) -> Self {
        Self {
            keychain,
            vault: Mutex::new(None),
        }
    }

    pub fn save(&self, id: &str, password: &str) -> AppResult<()> {
        Self::check_id(id)?;
        let mut guard = self.vault.lock();
        let vault = self.open(&mut guard)?;
        if vault.get(id).is_some_and(|saved| saved == password) {
            return Ok(());
        }
        let mut changed = vault.clone();
        changed.insert(id.to_owned(), password.to_owned());
        self.write(vault, changed)
    }

    /// Gives an empty string when no password is saved.
    pub fn load(&self, id: &str) -> AppResult<String> {
        Self::check_id(id)?;
        let mut guard = self.vault.lock();
        let vault = self.open(&mut guard)?;
        if let Some(password) = vault.get(id) {
            return Ok(password.clone());
        }
        // Not in the vault. An older version may have saved it as its own item.
        let Some(password) = self.keychain.get(id)? else {
            return Ok(String::new());
        };
        let mut changed = vault.clone();
        changed.insert(id.to_owned(), password.clone());
        self.write(vault, changed)?;
        // The password is safe in the vault now. A leftover old item does no harm.
        let _ = self.keychain.delete(id);
        Ok(password)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        Self::check_id(id)?;
        let mut guard = self.vault.lock();
        let vault = self.open(&mut guard)?;
        if vault.contains_key(id) {
            let mut changed = vault.clone();
            changed.remove(id);
            self.write(vault, changed)?;
        }
        // Also remove the item of an older version, if there is one.
        self.keychain.delete(id)
    }

    /// An id must not be the name of the vault item.
    /// Without this check the whole vault could be read or deleted as one password.
    fn check_id(id: &str) -> AppResult<()> {
        if id == KEYCHAIN_VAULT_ACCOUNT {
            return Err(ID_RESERVED.into());
        }
        Ok(())
    }

    /// Gives the vault. Reads it from the keychain the first time.
    /// A failed read is not kept, so the next call asks the keychain again.
    fn open<'vault>(&self, slot: &'vault mut Option<Vault>) -> AppResult<&'vault mut Vault> {
        let vault = match slot.take() {
            Some(vault) => vault,
            None => self.read()?,
        };
        Ok(slot.insert(vault))
    }

    fn read(&self) -> AppResult<Vault> {
        match self.keychain.get(KEYCHAIN_VAULT_ACCOUNT)? {
            Some(json) => serde_json::from_str(&json).map_err(|_| AppError::from(VAULT_BROKEN)),
            None => Ok(Vault::new()),
        }
    }

    /// Stores `changed` in the keychain. Only then it replaces the vault in memory.
    /// So a failed write leaves the memory as it was.
    fn write(&self, vault: &mut Vault, changed: Vault) -> AppResult<()> {
        let json = serde_json::to_string(&changed).map_err(|_| AppError::from(VAULT_BROKEN))?;
        self.keychain.set(KEYCHAIN_VAULT_ACCOUNT, &json)?;
        *vault = changed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    /// A keychain in memory. It counts how often the vault item is read.
    #[derive(Default)]
    struct FakeKeychain {
        items: Mutex<HashMap<String, String>>,
        vault_reads: AtomicUsize,
        fail_get: AtomicBool,
        fail_set: AtomicBool,
    }

    impl FakeKeychain {
        fn item(&self, account: &str) -> Option<String> {
            self.items.lock().get(account).cloned()
        }

        fn put(&self, account: &str, secret: &str) {
            self.items
                .lock()
                .insert(account.to_owned(), secret.to_owned());
        }
    }

    impl Keychain for FakeKeychain {
        fn get(&self, account: &str) -> AppResult<Option<String>> {
            if account == KEYCHAIN_VAULT_ACCOUNT {
                self.vault_reads.fetch_add(1, Ordering::Relaxed);
            }
            if self.fail_get.load(Ordering::Relaxed) {
                return Err("denied".into());
            }
            Ok(self.item(account))
        }

        fn set(&self, account: &str, secret: &str) -> AppResult<()> {
            if self.fail_set.load(Ordering::Relaxed) {
                return Err("denied".into());
            }
            self.put(account, secret);
            Ok(())
        }

        fn delete(&self, account: &str) -> AppResult<()> {
            self.items.lock().remove(account);
            Ok(())
        }
    }

    fn setup() -> (Arc<FakeKeychain>, PasswordRepository) {
        let keychain = Arc::new(FakeKeychain::default());
        let passwords = PasswordRepository::new(keychain.clone());
        (keychain, passwords)
    }

    #[test]
    fn many_loads_read_the_vault_one_time() {
        let (keychain, passwords) = setup();
        passwords.save("a", "one").unwrap();
        passwords.save("b", "two").unwrap();
        assert_eq!(passwords.load("a").unwrap(), "one");
        assert_eq!(passwords.load("b").unwrap(), "two");
        assert_eq!(passwords.load("missing").unwrap(), "");
        assert_eq!(keychain.vault_reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn delete_removes_the_password() {
        let (keychain, passwords) = setup();
        passwords.save("a", "one").unwrap();
        passwords.delete("a").unwrap();
        passwords.delete("a").unwrap();
        assert_eq!(passwords.load("a").unwrap(), "");
        assert_eq!(keychain.item(KEYCHAIN_VAULT_ACCOUNT).unwrap(), "{}");
    }

    #[test]
    fn delete_of_a_missing_id_does_not_write() {
        let (keychain, passwords) = setup();
        keychain.fail_set.store(true, Ordering::Relaxed);
        passwords.delete("missing").unwrap();
    }

    #[test]
    fn an_old_item_moves_into_the_vault() {
        let (keychain, passwords) = setup();
        keychain.put("old", "secret");
        assert_eq!(passwords.load("old").unwrap(), "secret");
        assert_eq!(keychain.item("old"), None);
        assert_eq!(passwords.load("old").unwrap(), "secret");
        assert!(keychain.item(KEYCHAIN_VAULT_ACCOUNT).is_some());
    }

    #[test]
    fn delete_removes_an_old_item() {
        let (keychain, passwords) = setup();
        keychain.put("old", "secret");
        passwords.delete("old").unwrap();
        assert_eq!(keychain.item("old"), None);
        assert_eq!(passwords.load("old").unwrap(), "");
    }

    #[test]
    fn a_failed_read_is_not_kept() {
        let (keychain, passwords) = setup();
        keychain.put(KEYCHAIN_VAULT_ACCOUNT, r#"{"a":"one"}"#);
        keychain.fail_get.store(true, Ordering::Relaxed);
        assert!(passwords.load("a").is_err());
        keychain.fail_get.store(false, Ordering::Relaxed);
        assert_eq!(passwords.load("a").unwrap(), "one");
        assert_eq!(keychain.vault_reads.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_failed_write_leaves_the_memory_unchanged() {
        let (keychain, passwords) = setup();
        passwords.save("a", "one").unwrap();
        keychain.fail_set.store(true, Ordering::Relaxed);
        assert!(passwords.save("a", "two").is_err());
        assert!(passwords.save("b", "new").is_err());
        assert!(passwords.delete("a").is_err());
        assert_eq!(passwords.load("a").unwrap(), "one");
        assert_eq!(passwords.load("b").unwrap(), "");
    }

    #[test]
    fn a_new_repository_reads_the_saved_passwords() {
        let (keychain, passwords) = setup();
        passwords.save("a", "one").unwrap();
        let again = PasswordRepository::new(keychain);
        assert_eq!(again.load("a").unwrap(), "one");
    }

    #[test]
    fn a_broken_vault_is_an_error_and_is_not_wiped() {
        let (keychain, passwords) = setup();
        keychain.put(KEYCHAIN_VAULT_ACCOUNT, "not json");
        assert!(passwords.load("a").is_err());
        assert!(passwords.save("a", "one").is_err());
        assert_eq!(keychain.item(KEYCHAIN_VAULT_ACCOUNT).unwrap(), "not json");
    }

    #[test]
    fn the_vault_name_is_not_an_id() {
        let (keychain, passwords) = setup();
        passwords.save("a", "one").unwrap();
        assert!(passwords.load(KEYCHAIN_VAULT_ACCOUNT).is_err());
        assert!(passwords.save(KEYCHAIN_VAULT_ACCOUNT, "x").is_err());
        assert!(passwords.delete(KEYCHAIN_VAULT_ACCOUNT).is_err());
        assert!(keychain.item(KEYCHAIN_VAULT_ACCOUNT).is_some());
    }
}
