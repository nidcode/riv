use crate::auth::{FileTokenStore, TokenStore, jwt, oauth};
use crate::error::{Result, RivError};
use crate::i18n::t;

pub async fn login() -> Result<i32> {
    let creds = oauth::login(&oauth::oauth_base(), |url| {
        println!("{}", t("Open this URL in your browser to sign in:"));
        println!("{url}");
    })
    .await?;
    FileTokenStore::default_location().save(&creds)?;
    println!("{}", t("Signed in."));
    Ok(0)
}

pub async fn logout() -> Result<i32> {
    let store = FileTokenStore::default_location();
    if let Some(c) = store.load()?
        && let Some(rt) = c.refresh_token
        && let Err(e) = oauth::revoke(&oauth::oauth_base(), &rt).await
    {
        eprintln!("warning: {e}");
    }
    store.clear()?;
    println!("{}", t("Signed out."));
    println!("To end your Builder ID session too, visit: https://idp.awsevents.com/oidc/logout");
    Ok(0)
}

pub fn whoami() -> Result<i32> {
    let Some(c) = FileTokenStore::default_location().load()? else {
        return Err(RivError::auth(t("Not signed in. Run `riv login`.")));
    };
    let email = c
        .id_token
        .as_deref()
        .and_then(|t| jwt::claim_str(t, "email"))
        .map(|e| jwt::mask_email(&e))
        .unwrap_or_else(|| t("unknown"));
    println!("{email}");
    Ok(0)
}
