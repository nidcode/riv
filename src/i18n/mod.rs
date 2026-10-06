//! en/ja message lookup. The key IS the English text; `en` returns it unchanged,
//! `ja` maps it through the dictionary (falling back to English when missing).

mod ja;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ja,
}

/// Pure resolution order: `RIV_LANG` > saved config > OS locale (`LC_ALL`, `LANG`) > English.
pub fn resolve_lang(env_riv: Option<&str>, config: Option<&str>, os: Option<&str>) -> Lang {
    let pick = [env_riv, config, os].into_iter().flatten().find(|v| !v.is_empty());
    match pick {
        Some(v) if v.to_lowercase().starts_with("ja") => Lang::Ja,
        _ => Lang::En,
    }
}

pub fn lang() -> Lang {
    let os = std::env::var("LC_ALL").ok().filter(|v| !v.is_empty()).or_else(|| std::env::var("LANG").ok());
    resolve_lang(
        std::env::var("RIV_LANG").ok().as_deref(),
        crate::config::Config::load().lang.as_deref(),
        os.as_deref(),
    )
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
    fn resolution_order() {
        assert_eq!(resolve_lang(Some("en"), Some("ja"), Some("ja_JP.UTF-8")), Lang::En, "env beats config");
        assert_eq!(resolve_lang(None, Some("ja"), Some("en_US.UTF-8")), Lang::Ja, "config beats OS");
        assert_eq!(resolve_lang(None, None, Some("ja_JP.UTF-8")), Lang::Ja);
        assert_eq!(resolve_lang(Some(""), None, None), Lang::En);
    }

    #[test]
    fn dictionary_has_unique_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in ja::ENTRIES {
            assert!(seen.insert(*k), "duplicate key {k}");
        }
    }
}
