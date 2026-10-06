use super::ConfigAction;
use crate::config::{Config, path};
use crate::error::Result;
use crate::i18n::{Lang, lang};

pub fn run(action: ConfigAction) -> Result<i32> {
    let p = path();
    let mut c = Config::load_from(&p);
    match action {
        ConfigAction::Set { key, value } => {
            c.set(&key, &value)?;
            c.save_to(&p)?;
            println!("{key} = {value}  ({})", p.display());
        }
        ConfigAction::Unset { key } => {
            c.unset(&key)?;
            c.save_to(&p)?;
            println!("{key} unset");
        }
        ConfigAction::Get { key } => println!("{}", c.get(&key)?.unwrap_or_default()),
        ConfigAction::Show => {
            let effective = if lang() == Lang::Ja { "ja" } else { "en" };
            let env = std::env::var("RIV_LANG").ok().filter(|v| !v.is_empty());
            println!("file: {}", p.display());
            println!("lang (saved): {}", c.lang.as_deref().unwrap_or("(not set)"));
            println!(
                "lang (effective): {effective}{}",
                env.map(|e| format!("  [RIV_LANG={e} overrides]")).unwrap_or_default()
            );
        }
    }
    Ok(0)
}
