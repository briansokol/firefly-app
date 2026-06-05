use crate::error::{AppError, Result};

const SERVICE: &str = "net.nsokol.firefly";
const ACCOUNT: &str = "litellm-device-token";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(map_keyring_err)
}

// A missing or locked OS keychain surfaces as a storage/platform failure rather
// than NoEntry. Turn that into actionable guidance instead of a raw dbus error.
fn map_keyring_err(e: keyring::Error) -> AppError {
    match e {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            AppError::KeychainUnavailable(
                "secure storage is unavailable. On Linux, start a Secret Service \
                 provider (e.g. gnome-keyring-daemon or KWallet) and unlock your \
                 login keyring, then try again."
                    .into(),
            )
        }
        other => AppError::Keychain(other),
    }
}

pub fn set_token(token: &str) -> Result<()> {
    entry()?.set_password(token).map_err(map_keyring_err)?;
    Ok(())
}

/// Read the device token from the OS keychain. Never exposed to the webview.
pub fn get_token() -> Result<String> {
    match entry()?.get_password() {
        Ok(t) if !t.is_empty() => Ok(t),
        Ok(_) => Err(AppError::NoToken),
        Err(keyring::Error::NoEntry) => Err(AppError::NoToken),
        Err(e) => Err(map_keyring_err(e)),
    }
}

pub fn has_token() -> bool {
    get_token().is_ok()
}
