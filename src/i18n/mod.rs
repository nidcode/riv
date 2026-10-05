//! en/ja message lookup. The key IS the English text; `en` returns it unchanged,
//! `ja` maps it through the dictionary (falling back to English when missing).

mod ja;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ja,
}

pub fn lang() -> Lang {
    let v = std::env::var("RIV_LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_lowercase();
    if v.starts_with("ja") { Lang::Ja } else { Lang::En }
}

pub fn t_in(lang: Lang, key: &str) -> String {
    match lang {
        Lang::En => key.to_string(),
        Lang::Ja => ja::lookup(key).unwrap_or(key).to_string(),
    }
}

pub fn t(key: &str) -> String {
    t_in(lang(), key)
}

/// Translate then substitute `{name}` placeholders.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        assert_eq!(t_in(Lang::En, "Reserved"), "Reserved");
        assert_eq!(t_in(Lang::Ja, "Reserved"), "予約済み");
        assert_eq!(t_in(Lang::Ja, "no such key"), "no such key");
    }

    #[test]
    fn dictionary_has_unique_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in ja::ENTRIES {
            assert!(seen.insert(*k), "duplicate key {k}");
        }
    }
}
