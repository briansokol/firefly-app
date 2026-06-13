use crate::error::{AppError, Result};

const SERVICE: &str = "net.nsokol.firefly";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(map_keyring_err)
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

fn device_account(user_id: &str) -> String {
    format!("device-token:{user_id}")
}
fn litellm_account(user_id: &str) -> String {
    format!("litellm-key:{user_id}")
}

fn read(account: &str) -> Result<String> {
    match entry(account)?.get_password() {
        Ok(t) if !t.is_empty() => Ok(t),
        Ok(_) => Err(AppError::NoToken),
        Err(keyring::Error::NoEntry) => Err(AppError::NoToken),
        Err(e) => Err(map_keyring_err(e)),
    }
}

pub fn set_device_token(user_id: &str, token: &str) -> Result<()> {
    entry(&device_account(user_id))?
        .set_password(token)
        .map_err(map_keyring_err)
}

/// Read a user's sync device token. Never exposed to the webview.
pub fn get_device_token(user_id: &str) -> Result<String> {
    read(&device_account(user_id))
}

pub fn set_litellm_key(user_id: &str, key: &str) -> Result<()> {
    entry(&litellm_account(user_id))?
        .set_password(key)
        .map_err(map_keyring_err)
}

/// Read a user's per-user LiteLLM virtual key. Never exposed to the webview.
pub fn get_litellm_key(user_id: &str) -> Result<String> {
    read(&litellm_account(user_id))
}

fn delete(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_keyring_err(e)),
    }
}

pub fn delete_device_token(user_id: &str) -> Result<()> {
    delete(&device_account(user_id))
}

pub fn delete_litellm_key(user_id: &str) -> Result<()> {
    delete(&litellm_account(user_id))
}
