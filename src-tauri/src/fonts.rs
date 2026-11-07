//! The fonts installed on the machine, for the font settings.

use std::collections::BTreeSet;

/// The family names of all installed fonts, sorted and without doubles.
/// Names that start with a dot are hidden system fonts of macOS. CSS cannot use them, so they are left out.
pub fn installed() -> Vec<String> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let names: BTreeSet<String> = database
        .faces()
        .filter_map(|face| face.families.first())
        .map(|(name, _)| name.trim().to_owned())
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .collect();
    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::installed;

    #[test]
    fn the_list_is_sorted_and_has_no_doubles() {
        let names = installed();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
