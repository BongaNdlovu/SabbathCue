//! Modern Bible names and their KJV spellings.
//!
//! The bundled public-domain corpus uses KJV-family wording. Spoken
//! transcripts normally use modern names, so FTS query expansion needs a
//! small curated bridge for names whose KJV spelling differs materially.

const MODERN_TO_KJV: &[(&str, &[&str])] = &[
    ("noah", &["noe"]),
    ("elijah", &["elias"]),
    ("isaiah", &["esaias"]),
    ("jonah", &["jonas"]),
    ("hosea", &["osee"]),
    ("joshua", &["josue"]),
    ("elisha", &["eliseus"]),
    ("jeremiah", &["jeremy"]),
    ("zechariah", &["zacharias"]),
    ("hezekiah", &["ezechias"]),
    ("uzziah", &["ozias"]),
    ("judah", &["juda"]),
    ("korah", &["core"]),
    ("zerubbabel", &["zorobabel"]),
    ("melchizedek", &["melchisedec"]),
    ("rahab", &["rachab"]),
    ("boaz", &["booz"]),
    ("hagar", &["agar"]),
    ("sarah", &["sara"]),
    ("abijah", &["abia"]),
    ("kish", &["cis"]),
    ("sinai", &["sina"]),
];

/// Return KJV spellings for a modern spoken name, if this term has aliases.
pub fn kjv_variants(term: &str) -> &'static [&'static str] {
    let lowered = term.to_ascii_lowercase();
    MODERN_TO_KJV
        .iter()
        .find(|(modern, _)| *modern == lowered)
        .map_or(&[], |(_, variants)| *variants)
}

#[cfg(test)]
mod tests {
    use super::kjv_variants;

    #[test]
    fn expands_modern_names_to_kjv_spellings() {
        assert_eq!(kjv_variants("Noah"), &["noe"]);
        assert_eq!(kjv_variants("ISAIAH"), &["esaias"]);
        assert_eq!(kjv_variants("Joshua"), &["josue"]);
    }

    #[test]
    fn leaves_unknown_and_existing_kjv_terms_unchanged() {
        assert!(kjv_variants("shepherd").is_empty());
        assert!(kjv_variants("Noe").is_empty());
    }
}
