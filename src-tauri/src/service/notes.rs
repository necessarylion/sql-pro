//! The notes about one database for the AI: a `claude.md` file in its own folder,
//! `<base>/<database>-<hash>/claude.md`. The hash keeps the databases of the same name on
//! two servers apart. The user edits it in the app. The AI adds facts to it.

use std::path::{Path, PathBuf};

use crate::constants::{NO_DATABASE_FOR_NOTES, NOTES_FILE, NOTES_FULL, NOTES_MAX_BYTES};
use crate::error::{AppError, AppResult};
use crate::model::Driver;

/// The folder name for the database: `<safe name>-<8 hex>`. The hex is a hash of the
/// driver, server host, port and database, so the same database name on two servers gets
/// two folders. The user is not in it, so two logins to one database share the notes.
/// For `SQLite` the hash uses the full file path, and host and port are not used.
pub(super) fn folder_name(
    driver: Driver,
    host: &str,
    port: u16,
    database: &str,
) -> AppResult<String> {
    let key = if driver == Driver::Sqlite {
        format!("{driver:?}\n{database}")
    } else {
        format!("{driver:?}\n{host}\n{port}\n{database}")
    };
    let hash = fnv1a(key.as_bytes()) & 0xffff_ffff;
    Ok(format!("{}-{hash:08x}", safe_name(driver, database)?))
}

/// FNV-1a, 64 bit. Unlike `DefaultHasher`, it gives the same value in every Rust version.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A safe, readable folder name for the database. For `SQLite` the database is a file path,
/// so its file name without the extension is used. Characters that a path could use
/// to leave the folder become `_`, and so does a `.` at the start.
fn safe_name(driver: Driver, database: &str) -> AppResult<String> {
    let name = if driver == Driver::Sqlite {
        Path::new(database)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        database.to_owned()
    };
    let name: String = name
        .trim()
        .chars()
        .map(|char| {
            if char.is_control() || "/\\:*?\"<>|".contains(char) {
                '_'
            } else {
                char
            }
        })
        .collect();
    // A leading `.` would make `.` and `..`, or a hidden folder.
    let dots = name.len() - name.trim_start_matches('.').len();
    let name = format!("{}{}", "_".repeat(dots), &name[dots..]);
    // Windows drops a `.` or space at the end of a folder name.
    let name = name.trim_end_matches(['.', ' ']);
    if name.is_empty() {
        return Err(NO_DATABASE_FOR_NOTES.into());
    }
    Ok(name.to_owned())
}

/// The path of the notes file of `folder` (from `folder_name`).
pub(super) fn path(base: &Path, folder: &str) -> PathBuf {
    base.join(folder).join(NOTES_FILE)
}

/// The notes text. Creates the folder and an empty file when they are not there yet.
pub(super) fn read(base: &Path, folder: &str) -> AppResult<String> {
    let file = path(base, folder);
    if !file.exists() {
        std::fs::create_dir_all(base.join(folder))?;
        std::fs::write(&file, "")?;
    }
    Ok(std::fs::read_to_string(file)?)
}

/// Writes the whole notes text. Text over `NOTES_MAX_BYTES` is refused.
pub(super) fn write(base: &Path, folder: &str, text: &str) -> AppResult<()> {
    if text.len() > NOTES_MAX_BYTES {
        return Err(AppError::Message(format!(
            "The notes are too long. They can hold at most {NOTES_MAX_BYTES} bytes."
        )));
    }
    std::fs::create_dir_all(base.join(folder))?;
    Ok(std::fs::write(path(base, folder), text)?)
}

/// Adds `fact` as a new line `- fact`. When the notes would get too long, nothing is written.
pub(super) fn append(base: &Path, folder: &str, fact: &str) -> AppResult<()> {
    let old = read(base, folder)?;
    let old = old.trim_end();
    let text = if old.is_empty() {
        format!("- {fact}\n")
    } else {
        format!("{old}\n- {fact}\n")
    };
    if text.len() > NOTES_MAX_BYTES {
        return Err(NOTES_FULL.into());
    }
    write(base, folder, &text)
}

#[cfg(test)]
mod tests {
    use super::{append, folder_name, path, read, safe_name, write};
    use crate::constants::NOTES_MAX_BYTES;
    use crate::model::Driver;

    #[test]
    fn folder_names_are_unique_per_server() {
        let name = |host: &str, port: u16, database: &str| {
            folder_name(Driver::Mysql, host, port, database).unwrap()
        };
        // A fixed input gives a fixed name. This catches a change of the hash by mistake.
        assert_eq!(name("db.example.com", 3306, "app"), "app-88a8a717");
        assert_ne!(
            name("one.example.com", 3306, "app"),
            name("two.example.com", 3306, "app")
        );
        assert_ne!(name("db", 3306, "app"), name("db", 3307, "app"));
        assert_ne!(
            name("db", 3306, "app"),
            folder_name(Driver::Postgres, "db", 3306, "app").unwrap()
        );
        assert!(name("db", 3306, "../etc").starts_with("___etc-"));

        let sqlite = |path: &str| folder_name(Driver::Sqlite, "", 0, path).unwrap();
        assert!(sqlite("/a/shop.db").starts_with("shop-"));
        assert_ne!(sqlite("/a/shop.db"), sqlite("/b/shop.db"));
    }

    #[test]
    fn folder_names_stay_inside() {
        let name = |database: &str| safe_name(Driver::Mysql, database);
        assert_eq!(name("shop").unwrap(), "shop");
        assert_eq!(name("../etc").unwrap(), "___etc");
        assert_eq!(name("a/b").unwrap(), "a_b");
        assert_eq!(name("a\\b\0c").unwrap(), "a_b_c");
        assert_eq!(name(".hidden").unwrap(), "_hidden");
        assert_eq!(name("..").unwrap(), "__");
        assert!(name("").is_err());
        assert!(name("  ").is_err());
        assert_eq!(
            safe_name(Driver::Sqlite, "/Users/me/data/shop.db").unwrap(),
            "shop"
        );
        assert!(safe_name(Driver::Sqlite, "/").is_err());
    }

    #[test]
    fn read_write_and_append() {
        let base = std::env::temp_dir().join(format!("sqlpro-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        assert_eq!(read(&base, "shop").unwrap(), "");
        assert!(path(&base, "shop").exists());

        write(&base, "shop", "status 1 = pending").unwrap();
        append(&base, "shop", "status 2 = done").unwrap();
        assert_eq!(
            read(&base, "shop").unwrap(),
            "status 1 = pending\n- status 2 = done\n"
        );

        assert!(write(&base, "shop", &"a".repeat(NOTES_MAX_BYTES + 1)).is_err());
        write(&base, "shop", &"a".repeat(NOTES_MAX_BYTES - 2)).unwrap();
        assert!(append(&base, "shop", "one more").is_err());
        assert_eq!(read(&base, "shop").unwrap().len(), NOTES_MAX_BYTES - 2);

        let _ = std::fs::remove_dir_all(&base);
    }
}
