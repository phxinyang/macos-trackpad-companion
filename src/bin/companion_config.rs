//! Small JSON/TOML bridge used by the macOS SwiftUI settings app.
//!
//! The GUI invokes this process for short-lived `dump` and `set` operations.
//! Keeping TOML ownership here means TUI, GUI, and companion-net all use the
//! same parser and atomic writer.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use macos_trackpad_companion::config;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "companion-config",
    about = "JSON bridge for companion TOML settings"
)]
struct Args {
    /// TOML path. Defaults to the same path used by companion-net.
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the parsed config and resolved path as JSON.
    Dump,
    /// Set one dotted TOML path. Scalars are inferred; objects/arrays use TOML syntax.
    Set {
        #[arg(long)]
        path: String,
        #[arg(long)]
        value: String,
    },
    /// Report machine-readable configuration and environment diagnostics.
    Doctor,
    /// Create a random LAN pairing token when the config has none.
    EnsureToken,
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Dump => dump(args.config.as_deref()),
        Command::Set { path, value } => set(args.config.as_deref(), &path, &value),
        Command::Doctor => doctor(args.config.as_deref()),
        Command::EnsureToken => ensure_token(args.config.as_deref()),
    }
}

fn ensure_token(path: Option<&Path>) -> Result<()> {
    let resolved = path.map(PathBuf::from).unwrap_or_else(config::default_path);
    let mut root = if resolved.exists() {
        let source = std::fs::read_to_string(&resolved)
            .with_context(|| format!("read config {}", resolved.display()))?;
        toml::from_str::<toml::Value>(&source).context("parse config for token")?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    let existing = root
        .get("net")
        .and_then(|net| net.get("token"))
        .and_then(toml::Value::as_str)
        .filter(|token| !token.is_empty());
    if existing.is_some() {
        println!(
            "{}",
            serde_json::to_string(&json!({
                "path": resolved,
                "created": false,
                "token_configured": true,
            }))?
        );
        return Ok(());
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("generate secure pairing token: {error:?}"))?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    set_value(&mut root, &["net", "token"], toml::Value::String(token));
    write_config(&resolved, &root)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "path": resolved,
            "created": true,
            "token_configured": true,
        }))?
    );
    Ok(())
}

fn doctor(path: Option<&Path>) -> Result<()> {
    let resolved = path.map(PathBuf::from).unwrap_or_else(config::default_path);
    let exists = resolved.exists();
    let parsed = config::load(path);
    let (config_ok, details) = match parsed {
        Ok((cfg, _)) => (
            true,
            json!({
                "listen_ip": cfg.net.listen_ip,
                "port": cfg.net.port,
                "web_enabled": cfg.net.web_enabled,
                "phone_enabled": cfg.net.phone_enabled,
                "token_configured": cfg.net.token.as_ref().is_some_and(|token| !token.is_empty()),
                "sync_system_settings": cfg.macos.sync_system_settings,
                "haptic_feedback": format!("{:?}", cfg.macos.haptic_feedback),
            }),
        ),
        Err(error) => (false, json!({ "error": error.to_string() })),
    };
    let mut report = json!({
        "config_path": resolved,
        "config_exists": exists,
        "config_ok": config_ok,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "network": details,
    });
    #[cfg(target_os = "macos")]
    {
        let prefs = macos_trackpad_companion::macos_preferences::read_raw();
        report["macos_preferences"] = json!({
            "trackpad_domain_available": prefs.trackpad_domain_available(),
            "global_domain_available": prefs.global_domain_available(),
            "raw_values": prefs.value_count(),
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        report["macos_preferences"] = json!({ "available": false });
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn dump(path: Option<&Path>) -> Result<()> {
    let (cfg, resolved) = config::load(path)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "path": resolved,
            "config": cfg,
        }))?
    );
    Ok(())
}

fn set(path: Option<&Path>, dotted: &str, raw_value: &str) -> Result<()> {
    let resolved = path.map(PathBuf::from).unwrap_or_else(config::default_path);
    let mut root = if resolved.exists() {
        let source = std::fs::read_to_string(&resolved)
            .with_context(|| format!("read config {}", resolved.display()))?;
        toml::from_str::<toml::Value>(&source).context("parse config for set")?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    let segments: Vec<&str> = dotted.split('.').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() {
        bail!("path must contain at least one non-empty segment");
    }
    if dotted == "scroll.modifier_zoom_mask" && raw_value == "0" {
        remove_value(&mut root, &segments);
    } else {
        let value = parse_value(raw_value)?;
        set_value(&mut root, &segments, value);
    }
    write_config(&resolved, &root)?;
    println!(
        "{}",
        serde_json::to_string(&json!({ "path": resolved, "updated": dotted }))?
    );
    Ok(())
}

fn write_config(resolved: &Path, root: &toml::Value) -> Result<()> {
    let rendered = toml::to_string_pretty(root).context("render config")?;
    let tmp = resolved.with_extension("toml.tmp");
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }
    std::fs::write(&tmp, rendered).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &resolved)
        .with_context(|| format!("replace config {}", resolved.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&resolved, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect config {}", resolved.display()))?;
    }
    Ok(())
}

fn parse_value(raw: &str) -> Result<toml::Value> {
    if raw == "true" || raw == "false" {
        return Ok(toml::Value::Boolean(raw == "true"));
    }
    if let Ok(value) = raw.parse::<i64>() {
        return Ok(toml::Value::Integer(value));
    }
    if let Ok(value) = raw.parse::<f64>() {
        return Ok(toml::Value::Float(value));
    }
    if raw.starts_with('{') || raw.starts_with('[') {
        let source = format!("value = {raw}");
        let table =
            toml::from_str::<toml::Value>(&source).context("parse TOML object/array value")?;
        return table.get("value").cloned().context("parsed value missing");
    }
    Ok(toml::Value::String(raw.to_string()))
}

fn set_value(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }
    if !root.is_table() {
        *root = toml::Value::Table(toml::map::Map::new());
    }
    let table = root.as_table_mut().expect("table initialized");
    if path.len() == 1 {
        table.insert(path[0].into(), value);
        return;
    }
    let child = table
        .entry(path[0])
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    set_value(child, &path[1..], value);
}

fn remove_value(root: &mut toml::Value, path: &[&str]) {
    let Some(table) = root.as_table_mut() else {
        return;
    };
    if path.len() == 1 {
        table.remove(path[0]);
        return;
    }
    if let Some(child) = table.get_mut(path[0]) {
        remove_value(child, &path[1..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalar_values() {
        assert_eq!(parse_value("true").unwrap().as_bool(), Some(true));
        assert_eq!(parse_value("28.5").unwrap().as_float(), Some(28.5));
        assert_eq!(parse_value("on").unwrap().as_str(), Some("on"));
    }

    #[test]
    fn sets_dotted_paths() {
        let mut root = toml::Value::Table(toml::map::Map::new());
        set_value(
            &mut root,
            &["cursor", "sensitivity"],
            toml::Value::Float(30.0),
        );
        assert_eq!(root["cursor"]["sensitivity"].as_float(), Some(30.0));
    }

    #[test]
    fn generated_pairing_token_is_hex() {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).unwrap();
        let token = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
