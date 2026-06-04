use crate::error::{AppError, Result};

const SERVICE: &str = "net.nsokol.firefly";
const ACCOUNT: &str = "litellm-device-token";

fn entry() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, ACCOUNT)?)
}

pub fn set_token(token: &str) -> Result<()> {
    entry()?.set_password(token)?;
    Ok(())
}

/// Read the device token from the OS keychain. Never exposed to the webview.
pub fn get_token() -> Result<String> {
    match entry()?.get_password() {
        Ok(t) if !t.is_empty() => Ok(t),
        Ok(_) => Err(AppError::NoToken),
        Err(keyring::Error::NoEntry) => Err(AppError::NoToken),
        Err(e) => Err(e.into()),
    }
}

pub fn has_token() -> bool {
    get_token().is_ok()
}
