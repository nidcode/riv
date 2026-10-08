use crate::auth::{FileTokenStore, TokenStore, jwt, oauth};
use crate::error::{Result, RivError};
use crate::i18n::t;

/// Open the documented sign-out chain and wait for the browser to come back.
async fn browser_sign_out() -> Result<()> {
    oauth::browser_logout(&oauth::oauth_base(), &oauth::idp_base(), |url| {
        println!("{}", t("Opening the sign-out page; if the browser does not open, visit:"));
        println!("{url}");
    })
    .await?;
    println!("{}", t("Browser sessions cleared."));
    Ok(())
}

pub async fn login(switch_account: bool) -> Result<i32> {
    if switch_account {
        browser_sign_out().await?;
    }
    let creds = oauth::login(&oauth::oauth_base(), |url| {
        println!("{}", t("Open this URL in your browser to sign in:"));
        println!("{url}");
    })
    .await?;
    FileTokenStore::default_location().save(&creds)?;
    println!("{}", t("Signed in."));
    Ok(0)
}

pub async fn logout(browser: bool) -> Result<i32> {
    let store = FileTokenStore::default_location();
    if let Some(c) = store.load()?
        && let Some(rt) = c.refresh_token
        && let Err(e) = oauth::revoke(&oauth::oauth_base(), &rt).await
    {
        eprintln!("warning: {e}");
    }
    store.clear()?;
    println!("{}", t("Signed out."));
    if browser {
        browser_sign_out().await?;
    } else {
        // Without this the browser may sign you straight back in as the same account.
        println!(
            "{}",
            t(
                "Your browser may still be signed in to Builder ID. To switch accounts run `riv logout --browser` or `riv login --switch-account`."
            )
        );
    }
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
