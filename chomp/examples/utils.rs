#![allow(dead_code)]

use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use bitcoind_async_client::{Auth, Client};

pub fn format_blob(bytes: &[u8]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => hex::encode(bytes),
    }
}

fn first_env_var(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| env::var(name).ok())
}

fn scope_rpc_url_to_wallet(rpc_url: String, wallet_env_names: &[&str]) -> String {
    if rpc_url.contains("/wallet/") {
        return rpc_url;
    }

    let Some(wallet_name) = first_env_var(wallet_env_names) else {
        return rpc_url;
    };
    let wallet_name = wallet_name.trim_matches('/');
    if wallet_name.is_empty() {
        return rpc_url;
    }

    format!("{}/wallet/{}", rpc_url.trim_end_matches('/'), wallet_name)
}

fn resolve_cookie_file_path(cookie_path: String) -> Result<PathBuf> {
    let path = PathBuf::from(cookie_path);
    if path.is_file() {
        return Ok(path);
    }

    if !path.is_dir() {
        bail!(
            "Cookie path '{}' does not exist or is not a file/directory",
            path.display()
        );
    }

    let direct_cookie = path.join(".cookie");
    if direct_cookie.is_file() {
        return Ok(direct_cookie);
    }

    let mut nested_candidates = Vec::new();
    for entry in fs::read_dir(&path)
        .with_context(|| format!("Failed to read cookie directory '{}'", path.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "Failed to inspect an entry under cookie directory '{}'",
                path.display()
            )
        })?;
        if !entry
            .file_type()
            .with_context(|| format!("Failed to read file type for '{}'", entry.path().display()))?
            .is_dir()
        {
            continue;
        }

        let candidate = entry.path().join(".cookie");
        if candidate.is_file() {
            nested_candidates.push(candidate);
        }
    }

    match nested_candidates.len() {
        1 => Ok(nested_candidates.remove(0)),
        0 => bail!(
            "Cookie path '{}' is a directory, but no '.cookie' file was found inside it",
            path.display()
        ),
        _ => bail!(
            "Cookie path '{}' is a directory with multiple '.cookie' files beneath it; set the env var to the exact file path",
            path.display()
        ),
    }
}

pub fn build_bitcoin_rpc_client_from_env() -> Result<(Client, String)> {
    let rpc_url = scope_rpc_url_to_wallet(
        env::var("BITCOIND_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:18443".to_string()),
        &["BITCOIND_RPC_WALLET"],
    );

    let auth = match env::var("BITCOIND_COOKIE_FILE") {
        Ok(cookie_path) => Auth::CookieFile(resolve_cookie_file_path(cookie_path)?),
        Err(_) => {
            let rpc_user = env::var("BITCOIND_RPC_USER")
                .context("Set BITCOIND_RPC_USER/BITCOIND_RPC_PASSWORD or BITCOIND_COOKIE_FILE")?;
            let rpc_password = env::var("BITCOIND_RPC_PASSWORD")
                .context("Set BITCOIND_RPC_USER/BITCOIND_RPC_PASSWORD or BITCOIND_COOKIE_FILE")?;
            Auth::UserPass(rpc_user, rpc_password)
        }
    };

    let client = Client::new(rpc_url.clone(), auth, None, None, None)
        .context("Failed to create Bitcoin RPC client")?;
    Ok((client, rpc_url))
}

pub fn build_liquid_rpc_client_from_env() -> Result<(Client, String)> {
    let rpc_url = scope_rpc_url_to_wallet(
        first_env_var(&["LIQUID_RPC_URL", "ELEMENTSD_RPC_URL"])
            .unwrap_or_else(|| "http://127.0.0.1:18884".to_string()),
        &["LIQUID_RPC_WALLET", "ELEMENTSD_RPC_WALLET"],
    );

    let auth = match first_env_var(&["LIQUID_COOKIE_FILE", "ELEMENTSD_COOKIE_FILE"]) {
        Some(cookie_path) => Auth::CookieFile(resolve_cookie_file_path(cookie_path)?),
        None => {
            let user = first_env_var(&["LIQUID_RPC_USER", "ELEMENTSD_RPC_USER"]);
            let password = first_env_var(&["LIQUID_RPC_PASSWORD", "ELEMENTSD_RPC_PASSWORD"]);

            match (user, password) {
                (Some(user), Some(password)) => Auth::UserPass(user, password),
                _ => bail!(
                    "Set LIQUID_RPC_USER/LIQUID_RPC_PASSWORD, ELEMENTSD_RPC_USER/ELEMENTSD_RPC_PASSWORD, or one of the LIQUID/ELEMENTSD cookie file variables"
                ),
            }
        }
    };

    let client = Client::new(rpc_url.clone(), auth, None, None, None)
        .context("Failed to create Liquid RPC client")?;
    Ok((client, rpc_url))
}

pub async fn ensure_wallet_rpc_endpoint(
    client: &Client,
    chain_label: &str,
    rpc_url: &str,
    wallet_env_hint: &str,
) -> Result<()> {
    client
        .call_raw::<serde_json::Value>("getwalletinfo", &[])
        .await
        .with_context(|| {
            if rpc_url.contains("/wallet/") {
                format!(
                    "{chain_label} RPC URL '{rpc_url}' does not expose a usable wallet. Make sure that wallet is loaded and supports wallet RPCs"
                )
            } else {
                format!(
                    "{chain_label} RPC URL '{rpc_url}' is not wallet-scoped. Use a URL such as '{}/wallet/<wallet-name>' or set {}",
                    rpc_url.trim_end_matches('/'),
                    wallet_env_hint
                )
            }
        })?;
    Ok(())
}
