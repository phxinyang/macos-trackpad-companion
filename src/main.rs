//! macos-trackpad-companion — userspace bridge from a PTP HID device
//! (Windows Precision Touchpad / Microsoft Precision Touchpad) to native
//! macOS gesture events.
//!
//! On Linux and Windows, PTP devices are handled natively. macOS has no
//! built-in PTP consumer, so this process opens the device's digitizer
//! interface, decodes touch frames, and synthesizes CGEvents for cursor,
//! click, scroll, pinch, rotate, and 3+/4-finger swipe.
//!
//! Permissions: needs Input Monitoring (to read raw HID) and Accessibility
//! (to post CGEvents) the first run; macOS will prompt.
//!
//! Configuration: all tuning lives in a TOML file at
//! `$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml` (default
//! `~/.config/macos-trackpad-companion/config.toml`). The CLI surface
//! intentionally only carries `--config PATH` and `-v` — see `config.rs`
//! / README for the full schema.

#[cfg(target_os = "macos")]
mod app_context;
#[cfg(target_os = "macos")]
mod boot;
#[cfg(target_os = "macos")]
mod config;
#[cfg(target_os = "macos")]
mod descriptor;
#[cfg(target_os = "macos")]
mod gesture;
#[cfg(target_os = "macos")]
mod hid;
#[cfg(target_os = "macos")]
mod instance_lock;
#[cfg(target_os = "macos")]
mod macos_preferences;
#[cfg(target_os = "macos")]
mod output;
#[cfg(target_os = "macos")]
mod overlay;
#[cfg(target_os = "macos")]
mod report;
#[cfg(target_os = "macos")]
mod scan_clock;
#[cfg(target_os = "macos")]
mod scroll_policy;
#[cfg(target_os = "macos")]
mod time;

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use clap::Parser;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to TOML config. Default:
    /// `$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`
    /// (or `~/.config/macos-trackpad-companion/config.toml` if
    /// `XDG_CONFIG_HOME` is unset). Missing file → all defaults.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Verbose logging (-v debug, -vv trace). Overrides `[log].level`
    /// from the config file when set.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    // Block SIGINT/SIGTERM in the main thread *before* any other code
    // runs. Threads inherit the caller's signal mask at spawn time;
    // a clean Ctrl+C only fires `DeviceState::drop` (which writes the
    // firmware's "back to mouse" SET) if all threads have these
    // signals blocked, so the dedicated sigwait worker installed
    // later catches them. NSApplication, IOHIDManager, env_logger,
    // anything that calls into a framework that internally
    // pthread_create's must run after this.
    hid::block_shutdown_signals();

    let args = Args::parse();
    let (mut cfg, cfg_path) = config::load(args.config.as_deref())?;

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
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level.as_str()));
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

    let sync_report = macos_preferences::apply(&mut cfg);
    log::debug!(
        "macOS settings sync enabled={} raw={} applied={} overrides={} unsupported={}",
        sync_report.enabled,
        sync_report.raw_values,
        sync_report.applied.len(),
        sync_report.explicit_overrides.len(),
        sync_report.unsupported.len(),
    );

    if cfg_path.exists() {
        log::info!(
            "macos-trackpad-companion starting (config={})",
            cfg_path.display()
        );
    } else {
        log::info!(
            "macos-trackpad-companion starting (no config at {} — using defaults)",
            cfg_path.display(),
        );
    }
    log::debug!("resolved config summary: {:?}", cfg.log_summary());

    // Bound to a non-underscore name so the guard lives until end of
    // main; closing the fd releases the kernel's flock.
    let lock = instance_lock::acquire()?;
    log::debug!("acquired instance lock at {}", lock.path.display());

    let emitter = output::Emitter::new(boot::emitter_config(&cfg));
    let cursor_accel = boot::cursor_accel(&cfg);
    let mut manager = hid::Manager::new(hid::Filter {
        vid: cfg.device.vid,
        pid: cfg.device.pid,
    })
    .context("open IOHIDManager")?;

    let gesture_options = boot::gesture_options(&cfg);

    if cfg.overlay.enable {
        let overlay = overlay::Overlay::new(cfg.overlay.duration_ms);
        let wrapped = output::OverlayOutput::new(emitter, overlay);
        let mut state = gesture::State::with_options(wrapped, cursor_accel, gesture_options);
        manager.run(move |frame, ts| state.on_frame_at(frame, ts))?;
    } else {
        let mut state = gesture::State::with_options(emitter, cursor_accel, gesture_options);
        manager.run(move |frame, ts| state.on_frame_at(frame, ts))?;
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "macos-trackpad-companion requires macOS; run `cargo test --lib` for portable gesture tests"
    );
}
