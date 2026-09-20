use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};

const YOUTUBE_TOKEN: &str = "YOUTUBE_ACCESS_TOKEN";
const INSTAGRAM_TOKEN: &str = "INSTAGRAM_ACCESS_TOKEN";
const INSTAGRAM_ACCOUNT: &str = "INSTAGRAM_ACCOUNT_ID";
const TIKTOK_TOKEN: &str = "TIKTOK_ACCESS_TOKEN";
const TWITCH_TOKEN: &str = "TWITCH_ACCESS_TOKEN";
const TWITCH_CLIENT: &str = "TWITCH_CLIENT_ID";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredCredentials {
    access_token: String,
    account_id: Option<String>,
    client_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct StoredAccounts {
    #[serde(default)]
    accounts: BTreeMap<String, StoredCredentials>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectAccountRequest {
    access_token: String,
    account_id: Option<String>,
    client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    platform: String,
    connected: bool,
}

pub struct AccountManager {
    path: PathBuf,
    accounts: Mutex<StoredAccounts>,
}

impl AccountManager {
    fn load(path: PathBuf) -> Result<Self, String> {
        let accounts = if path.is_file() {
            let raw = fs::read_to_string(&path)
                .map_err(|error| format!("read publisher accounts: {error}"))?;
            serde_json::from_str(&raw)
                .map_err(|error| format!("parse publisher accounts: {error}"))?
        } else {
            StoredAccounts::default()
        };
        let manager = Self {
            path,
            accounts: Mutex::new(accounts),
        };
        manager.apply_environment()?;
        Ok(manager)
    }

    fn statuses(&self) -> Result<Vec<AccountStatus>, String> {
        let accounts = self
            .accounts
            .lock()
            .map_err(|_| "publisher account state is unavailable".to_owned())?;
        Ok(platforms()
            .into_iter()
            .map(|platform| AccountStatus {
                platform: platform.to_owned(),
                connected: accounts.accounts.contains_key(platform)
                    || environment_has_credentials(platform),
            })
            .collect())
    }

    fn connect(
        &self,
        platform: &str,
        request: ConnectAccountRequest,
    ) -> Result<Vec<AccountStatus>, String> {
        validate_platform(platform)?;
        let credentials = normalize_credentials(platform, request)?;
        {
            let mut accounts = self
                .accounts
                .lock()
                .map_err(|_| "publisher account state is unavailable".to_owned())?;
            accounts.accounts.insert(platform.to_owned(), credentials);
            write_private_json(&self.path, &accounts)?;
            apply_accounts(&accounts);
        }
        self.statuses()
    }

    fn disconnect(&self, platform: &str) -> Result<Vec<AccountStatus>, String> {
        validate_platform(platform)?;
        {
            let mut accounts = self
                .accounts
                .lock()
                .map_err(|_| "publisher account state is unavailable".to_owned())?;
            accounts.accounts.remove(platform);
            write_private_json(&self.path, &accounts)?;
            clear_platform_environment(platform);
            apply_accounts(&accounts);
        }
        self.statuses()
    }

    fn apply_environment(&self) -> Result<(), String> {
        let accounts = self
            .accounts
            .lock()
            .map_err(|_| "publisher account state is unavailable".to_owned())?;
        apply_accounts(&accounts);
        Ok(())
    }
}

pub fn manager_for(app: &AppHandle) -> Result<AccountManager, Box<dyn std::error::Error>> {
    let directory = app.path().app_data_dir()?.join("clipfarmer");
    fs::create_dir_all(&directory)?;
    AccountManager::load(directory.join("publisher-accounts.json")).map_err(Into::into)
}

#[tauri::command]
pub fn list_publisher_accounts(
    state: State<'_, AccountManager>,
) -> Result<Vec<AccountStatus>, String> {
    state.statuses()
}

#[tauri::command]
pub fn connect_publisher_account(
    state: State<'_, AccountManager>,
    platform: String,
    credentials: ConnectAccountRequest,
) -> Result<Vec<AccountStatus>, String> {
    state.connect(&platform, credentials)
}

#[tauri::command]
pub fn disconnect_publisher_account(
    state: State<'_, AccountManager>,
    platform: String,
) -> Result<Vec<AccountStatus>, String> {
    state.disconnect(&platform)
}

fn platforms() -> [&'static str; 4] {
    ["youtube", "tiktok", "instagram", "twitch"]
}

fn validate_platform(platform: &str) -> Result<(), String> {
    if platforms().contains(&platform) {
        Ok(())
    } else {
        Err(format!("unsupported publishing platform {platform}"))
    }
}

fn normalize_credentials(
    platform: &str,
    request: ConnectAccountRequest,
) -> Result<StoredCredentials, String> {
    let access_token = request.access_token.trim().to_owned();
    if access_token.is_empty() {
        return Err("An access token is required.".to_owned());
    }
    let account_id = request.account_id.map(|value| value.trim().to_owned());
    let client_id = request.client_id.map(|value| value.trim().to_owned());
    if platform == "instagram" && account_id.as_deref().unwrap_or_default().is_empty() {
        return Err("An Instagram account ID is required.".to_owned());
    }
    if platform == "twitch" && client_id.as_deref().unwrap_or_default().is_empty() {
        return Err("A Twitch client ID is required.".to_owned());
    }
    Ok(StoredCredentials {
        access_token,
        account_id,
        client_id,
    })
}

fn apply_accounts(accounts: &StoredAccounts) {
    for (platform, credentials) in &accounts.accounts {
        match platform.as_str() {
            "youtube" => std::env::set_var(YOUTUBE_TOKEN, &credentials.access_token),
            "instagram" => {
                std::env::set_var(INSTAGRAM_TOKEN, &credentials.access_token);
                if let Some(account_id) = &credentials.account_id {
                    std::env::set_var(INSTAGRAM_ACCOUNT, account_id);
                }
            }
            "tiktok" => std::env::set_var(TIKTOK_TOKEN, &credentials.access_token),
            "twitch" => {
                std::env::set_var(TWITCH_TOKEN, &credentials.access_token);
                if let Some(client_id) = &credentials.client_id {
                    std::env::set_var(TWITCH_CLIENT, client_id);
                }
            }
            _ => {}
        }
    }
}

fn environment_has_credentials(platform: &str) -> bool {
    let populated = |name: &str| std::env::var(name).is_ok_and(|value| !value.trim().is_empty());
    match platform {
        "youtube" => populated(YOUTUBE_TOKEN),
        "instagram" => populated(INSTAGRAM_TOKEN) && populated(INSTAGRAM_ACCOUNT),
        "tiktok" => populated(TIKTOK_TOKEN),
        "twitch" => populated(TWITCH_TOKEN) && populated(TWITCH_CLIENT),
        _ => false,
    }
}

fn clear_platform_environment(platform: &str) {
    let names: &[&str] = match platform {
        "youtube" => &[YOUTUBE_TOKEN],
        "instagram" => &[INSTAGRAM_TOKEN, INSTAGRAM_ACCOUNT],
        "tiktok" => &[TIKTOK_TOKEN],
        "twitch" => &[TWITCH_TOKEN, TWITCH_CLIENT],
        _ => &[],
    };
    for name in names {
        std::env::remove_var(name);
    }
}

fn write_private_json(path: &Path, accounts: &StoredAccounts) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "publisher account path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("create account directory: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(accounts)
        .map_err(|error| format!("serialize publisher accounts: {error}"))?;
    fs::write(&temporary, contents)
        .map_err(|error| format!("write publisher accounts: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("secure publisher accounts: {error}"))?;
    }
    fs::rename(&temporary, path).map_err(|error| format!("save publisher accounts: {error}"))
}
