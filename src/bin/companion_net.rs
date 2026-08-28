//! companion-net — network-input sibling of the `companion` daemon.
//!
//! Instead of opening a PTP HID device, it accepts touch frames from
//! the network (Android phone: UDP from the native app, WebSocket from
//! the browser touchpad page) and feeds the identical gesture→CGEvent
//! pipeline. The web page is served at `http://<this-mac>:<port>/` so
//! a browser alone can drive the Mac; wire format in
//! docs/wire-protocol.md.
//!
//! Permissions: needs **Accessibility** only — no Input Monitoring,
//! since there's no device to read. Unlike the daemon there is no
//! hardware to reset, so Ctrl+C terminates directly with no teardown
//! worker and no firmware "back to mouse" concern.
//!
//! Mutually exclusive with the HID daemon via the same instance lock:
//! two live input sources would both drive one cursor.

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use clap::Parser;
#[cfg(target_os = "macos")]
use companion::{boot, config, gesture, instance_lock, net, output, overlay};
#[cfg(target_os = "macos")]
use macos_trackpad_companion as companion;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to TOML config. Default matches the companion daemon:
    /// `$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`,
    /// falling back to `~/.config/macos-trackpad-companion/config.toml`.
    /// Missing file → defaults (`[net]` port 4242 on all interfaces).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Verbose logging (-v debug, -vv trace). Overrides `[log].level`.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Override `[net].port` for this run (UDP and HTTP/WebSocket).
    #[arg(long, value_name = "PORT")]
    port: Option<u16>,

    /// Override `[net].listen_ip` for this run.
    #[arg(long, value_name = "IP")]
    listen_ip: Option<String>,

    /// Override `[net].token`. Enables bearer authentication for WebSocket
    /// and ATK1-wrapped UDP frames.
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    let args = Args::parse();
    let (mut cfg, cfg_path) = config::load(args.config.as_deref())?;
    if let Some(port) = args.port {
        cfg.net.port = port;
    }
    if args.listen_ip.is_some() {
        cfg.net.listen_ip = args.listen_ip;
    }
    if args.token.is_some() {
        cfg.net.token = args.token;
    }

    let level = if args.verbose > 0 {
        match args.verbose {
            1 => "debug",
            _ => "trace",
        }
        .to_string()
    } else {
        cfg.log.level.clone()
    };
    let mut log_builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level));
    log_builder.format_timestamp_millis();
    let log_file_path = cfg.log.file.as_deref().map(config::expand_tilde);
    if let Some(path) = log_file_path.as_deref() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create log dir {}", parent.display()))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open log file {}", path.display()))?;
        log_builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    log_builder.init();

    // The network client is a virtual touch surface. Physical-trackpad-only
    // settings such as `Clicking=0` must not disable phone tap-to-click;
    // explicit TOML remains authoritative.
    let sync_report = companion::macos_preferences::apply_for_virtual_input(&mut cfg);
    log::debug!(
        "macOS settings sync enabled={} raw={} applied={} overrides={} unsupported={}",
        sync_report.enabled,
        sync_report.raw_values,
        sync_report.applied.len(),
        sync_report.explicit_overrides.len(),
        sync_report.unsupported.len(),
    );

    // CGEventPost silently drops synthetic events without this grant,
    // which would look exactly like "the pipeline works but nothing
    // happens". Check after argument/config handling so --help and
    // malformed-config diagnostics remain usable without TCC access.
    ensure_accessibility()?;

    log::info!("companion-net starting (config={})", cfg_path.display());
    log::debug!("resolved config: {:#?}", cfg);

    let lock = instance_lock::acquire()?;
    log::debug!("acquired instance lock at {}", lock.path.display());

    let emitter = output::Emitter::new(boot::emitter_config(&cfg));
    let cursor_accel = boot::cursor_accel(&cfg);
    let options = boot::gesture_options(&cfg);

    if cfg.overlay.enable {
        let overlay = overlay::Overlay::new(cfg.overlay.duration_ms);
        let wrapped = output::OverlayOutput::new(emitter, overlay);
        let mut state = gesture::State::with_options(wrapped, cursor_accel, options);
        net::Server::new(cfg.net).run(&mut state)
    } else {
        let mut state = gesture::State::with_options(emitter, cursor_accel, options);
        net::Server::new(cfg.net).run(&mut state)
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("companion-net requires macOS; run `cargo test --lib` for portable gesture tests");
}

/// Returns `true` once this process may post synthetic events. With
/// `prompt = true` macOS shows its own "would like to control this
/// computer" dialog; the terminal app hosting this binary is what gets
/// TCC-attributed when the binary isn't a bundled .app.
#[cfg(target_os = "macos")]
fn ax_trusted(prompt: bool) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        // C Boolean is an unsigned char — not Rust's bool.
        fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> u8;
    }

    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::from(prompt);
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const _) != 0 }
}

#[cfg(target_os = "macos")]
fn ensure_accessibility() -> Result<()> {
    if ax_trusted(true) {
        return Ok(());
    }
    anyhow::bail!(
        "Accessibility permission required (synthetic CGEvents are silently \
         dropped without it):\n\
         1. Grant it in System Settings → Privacy & Security → Accessibility.\n\
         2. The dialog that just appeared targets the app running this\n\
            binary (your terminal). Add/enable THAT entry if companion-net\n\
            itself doesn't appear in the list.\n\
         3. Restart companion-net."
    )
}
