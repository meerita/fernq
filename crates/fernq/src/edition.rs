//! The Rust edition of a compilation.
//!
//! The edition is configuration: the command line provides it, and the stages
//! whose result depends on it receive it as a parameter. Fernq accepts the
//! four stable editions and has no default.

/// A stable Rust edition. Later editions compare greater.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Edition {
    E2015,
    E2018,
    E2021,
    E2024,
}

impl Edition {
    /// Returns the edition whose name is exactly `name`, such as `2021`.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "2015" => Some(Self::E2015),
            "2018" => Some(Self::E2018),
            "2021" => Some(Self::E2021),
            "2024" => Some(Self::E2024),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_four_stable_edition_names() {
        assert_eq!(Edition::from_name("2015"), Some(Edition::E2015));
        assert_eq!(Edition::from_name("2018"), Some(Edition::E2018));
        assert_eq!(Edition::from_name("2021"), Some(Edition::E2021));
        assert_eq!(Edition::from_name("2024"), Some(Edition::E2024));
    }

    #[test]
    fn rejects_every_other_name() {
        for name in [
            "", "2027", "2012", "future", "e2024", " 2024", "2024 ", "2O24", "24", "+2024",
        ] {
            assert_eq!(Edition::from_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn later_editions_compare_greater() {
        assert!(Edition::E2015 < Edition::E2018);
        assert!(Edition::E2018 < Edition::E2021);
        assert!(Edition::E2021 < Edition::E2024);
    }
}
