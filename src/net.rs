//! Network transport: feeds the same gesture pipeline as
//! [`crate::hid`] from UDP datagrams / WebSocket messages instead of a
//! PTP device. Wire format lives in `docs/wire-protocol.md`, decoded by
//! the `touchpad-proto` crate.
//!
//! Threading: two producers (UDP reader, TCP/WS readers) push decoded
//! frames into an mpsc channel; a single pump thread owns the callback,
//! preserving `gesture::State`'s single-threaded assumption and giving
//! one place to do scan-time mapping (`ScanTimeClock`, same as hid.rs)
//! and the idle-lift watchdog.
//!
//! Duplicate handling differs by source. UDP may reorder/retransmit:
//! in-order-forward is processed as-is, exact duplicates of *motion*
//! frames are dropped (the next frame supersedes them), but duplicates
//! of *lifted* frames pass through — clients retransmit gesture-end on
//! purpose because it's the only stateful transition; repeated empty
//! frames are benign to the engine. WebSocket sources are reliable +
//! ordered TCP streams, so everything arrives as Forward.

use crate::config::Net;
use crate::gesture;
use crate::output::Output;
use crate::report::{Contact, Frame};
use crate::scan_clock::ScanTimeClock;
use crate::time::Timestamp;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use touchpad_proto::DecodeError;

/// Without any frame for this long while contacts were down, inject an
/// all-lifted frame — packet loss stole the lift and taps would hang.
const IDLE_LIFT_AFTER: Duration = Duration::from_millis(250);
const PUMP_POLL: Duration = Duration::from_millis(50);
/// Heartbeat period for ingest statistics. Always logged when non-zero
/// so a silent terminal unambiguously means "no packets arriving".
const STATS_EVERY: Duration = Duration::from_secs(30);

/// Authenticated UDP envelope. The payload remains an unchanged ATP1 frame;
/// the envelope is only required when `[net].token` is configured.
const AUTH_MAGIC: [u8; 4] = *b"ATK1";
const AUTH_HEADER_LEN: usize = 6;
const AUTH_TOKEN_MAX_BYTES: usize = 256;

#[derive(Default)]
struct Stats {
    udp_datagrams: AtomicU64,
    decode_errors: AtomicU64,
    ws_frames: AtomicU64,
    consumed: AtomicU64,
}

enum Incoming {
    /// A newly accepted frame, still in wire form.
    Frame {
        peer: PeerId,
        frame: touchpad_proto::Frame,
    },
    /// Loss telemetry from the UDP path, only for logging.
    Gap { peer: PeerId, lost: u32 },
}

/// A transport endpoint is part of the gesture session identity.  UDP and
/// WebSocket sequence numbers are independent, and a late packet from a
/// previous client must never be interpreted as a continuation of the
/// current client's finger set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PeerTransport {
    Udp,
    WebSocket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PeerId {
    transport: PeerTransport,
    addr: SocketAddr,
}

/// Keep a source that just lost ownership from immediately taking the state
/// machine back when one of its in-flight packets arrives after a peer switch.
/// The interval is deliberately short: a client that reconnects after this
/// grace period is admitted as a fresh session.
const PEER_QUARANTINE: Duration = Duration::from_millis(600);

#[derive(Debug, PartialEq, Eq)]
enum PeerDecision {
    Accept { switched_from: Option<PeerId> },
    IgnoreRetired,
}

#[derive(Default)]
struct PeerGate {
    active: Option<PeerId>,
    retired: HashMap<PeerId, Instant>,
}

impl PeerGate {
    fn observe(&mut self, peer: PeerId, now: Instant) -> PeerDecision {
        self.retired.retain(|_, until| *until > now);
        if self.retired.contains_key(&peer) {
            return PeerDecision::IgnoreRetired;
        }
        if self.active == Some(peer) {
            return PeerDecision::Accept { switched_from: None };
        }
        let switched_from = self.active.replace(peer);
        if let Some(previous) = switched_from {
            self.retired.insert(previous, now + PEER_QUARANTINE);
        }
        PeerDecision::Accept { switched_from }
    }
}

/// The net transport pushes frames and idle-ticks into this sink.
/// Implementors own the gesture state; the idle tick is what fires
/// drag-lock releases (see `gesture::GestureOptions::release_delay_ms`).
pub trait InputSink {
    fn on_frame(&mut self, frame: Frame, ts: Timestamp);
    fn idle_tick(&mut self, now: Timestamp);
    /// The link went silent while contacts were still down. This is not
    /// a lift — the fingers may well still be on the pad — so the
    /// gesture is canceled rather than completed.
    fn on_link_timeout(&mut self, now: Timestamp);
}

impl<S: InputSink + ?Sized> InputSink for &mut S {
    fn on_frame(&mut self, frame: Frame, ts: Timestamp) {
        (**self).on_frame(frame, ts)
    }
    fn idle_tick(&mut self, now: Timestamp) {
        (**self).idle_tick(now)
    }
    fn on_link_timeout(&mut self, now: Timestamp) {
        (**self).on_link_timeout(now)
    }
}

impl<O: Output> InputSink for gesture::State<O> {
    fn on_frame(&mut self, frame: Frame, ts: Timestamp) {
        self.on_frame_at(frame, ts);
    }

    fn idle_tick(&mut self, now: Timestamp) {
        self.tick(now);
    }

    fn on_link_timeout(&mut self, now: Timestamp) {
        self.cancel_touch(now);
    }
}

pub struct Server {
    cfg: Net,
}

impl Server {
    pub fn new(cfg: Net) -> Self {
        Self { cfg }
    }

    /// Binds UDP+TCP on `[net].port` and pumps decoded frames into
    /// `sink` until the process exits (Ctrl+C kills us outright —
    /// unlike the HID path there is no hardware to reset).
    pub fn run(&mut self, sink: &mut dyn InputSink) -> Result<()> {
        let token = self
            .cfg
            .token
            .clone()
            .filter(|token| !token.is_empty());
        if let Some(token) = token.as_deref() {
            if token.len() > AUTH_TOKEN_MAX_BYTES {
                anyhow::bail!(
                    "[net] token is too long ({} bytes; maximum is {})",
                    token.len(),
                    AUTH_TOKEN_MAX_BYTES
                );
            }
            log::info!(
                "[net] bearer token enabled for WebSocket and UDP ATK1 envelopes"
            );
        }
        let bind_ip = self
            .cfg
            .listen_ip
            .clone()
            .unwrap_or_else(|| "0.0.0.0".into());
        let requested_port = self.cfg.port;

        let udp = UdpSocket::bind((bind_ip.as_str(), requested_port))
            .with_context(|| format!("bind UDP {bind_ip}:{requested_port}"))?;
        // Port 0 asks the kernel for an ephemeral port. Resolve it from
        // the UDP socket, then use that same number for TCP so the page
        // and frame endpoint still share one advertised port.
        let port = if requested_port == 0 {
            udp.local_addr().context("read assigned UDP port")?.port()
        } else {
            requested_port
        };
        let tcp = TcpListener::bind((bind_ip.as_str(), port))
            .with_context(|| format!("bind TCP {bind_ip}:{port}"))?;
        log::info!(
            "[net] listening on udp+tcp {bind_ip}:{port} — touchpad page at http://<this-mac>:{port}/"
        );

        let (tx, rx) = mpsc::channel::<Incoming>();
        let stats = Arc::new(Stats::default());

        std::thread::Builder::new()
            .name("net-udp".into())
            .spawn({
                let tx = tx.clone();
                let stats = Arc::clone(&stats);
                let token = token.clone();
                move || udp_reader(udp, tx, stats, token.as_deref())
            })
            .context("spawn udp reader")?;

        let static_page =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/touchpad.html"));
        let tester_page =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/tester.html"));
        let page_ctx = WebPageContext {
            html: static_page.to_string(),
            tester_html: tester_page.to_string(),
            port,
        };
        std::thread::Builder::new()
            .name("net-tcp".into())
            .spawn({
                let tx = tx.clone();
                let stats = Arc::clone(&stats);
                let token = token.clone();
                move || tcp_acceptor(tcp, tx, page_ctx, stats, token.as_deref())
            })
            .context("spawn tcp acceptor")?;

        std::thread::Builder::new()
            .name("net-stats".into())
            .spawn({
                let stats = Arc::clone(&stats);
                move || stats_loop(stats)
            })
            .context("spawn stats reporter")?;

        pump(rx, &stats, sink)
    }
}

fn stats_loop(stats: Arc<Stats>) {
    loop {
        std::thread::sleep(STATS_EVERY);
        // Unconditional heartbeat: a run of all-zero lines is just as
        // diagnostic as numbers — it proves the process lives while no
        // datagrams reach it.
        log::info!(
            "[net] stats: udp_rx={} decode_err={} ws={} engine_in={}",
            stats.udp_datagrams.load(Ordering::Relaxed),
            stats.decode_errors.load(Ordering::Relaxed),
            stats.ws_frames.load(Ordering::Relaxed),
            stats.consumed.load(Ordering::Relaxed),
        );
    }
}

// --- pump -------------------------------------------------------------------

fn pump(rx: mpsc::Receiver<Incoming>, stats: &Stats, sink: &mut dyn InputSink) -> Result<()> {
    let mut scan_clock = ScanTimeClock::new();
    let mut contacts_down = false;
    let mut last_arrival = Instant::now();
    let mut peers = PeerGate::default();

    loop {
        match rx.recv_timeout(PUMP_POLL) {
            Ok(Incoming::Gap { peer, lost }) => {
                if peers.active != Some(peer) {
                    continue;
                }
                log::warn!("[net] seq gap: {lost} frame(s) lost mid-stream");
            }
            Ok(Incoming::Frame { peer, frame: wire }) => {
                let now = Instant::now();
                let switched_from = match peers.observe(peer, now) {
                    PeerDecision::IgnoreRetired => {
                        log::debug!("[net] ignoring late frame from retired peer {peer:?}");
                        continue;
                    }
                    PeerDecision::Accept { switched_from } => switched_from,
                };
                if let Some(previous) = switched_from {
                    if contacts_down {
                        log::info!(
                            "[net] peer changed {previous:?} → {peer:?}; canceling active touch"
                        );
                        sink.on_link_timeout(Timestamp::now());
                    } else {
                        log::info!("[net] peer changed {previous:?} → {peer:?}");
                    }
                } else if peers.active == Some(peer) {
                    log::debug!("[net] active peer = {peer:?}");
                }
                if switched_from.is_some() {
                    // Scan clocks are sender-local. Reusing the previous
                    // offset would turn a new sender's arbitrary scan time
                    // into a large fake motion interval.
                    scan_clock = ScanTimeClock::new();
                }
                stats.consumed.fetch_add(1, Ordering::Relaxed);
                // Current state, not sticky: this exact flag drives the
                // idle-lift watchdog below.
                contacts_down = !wire.contacts.is_empty();
                // Same clock mapping as the HID bridge: per-frame deltas
                // follow the *sender's* scan-time deltas, immune to
                // network jitter between touch instant and arrival here.
                let aligned_ts = scan_clock.observe(wire.scan_time_u16(), Timestamp::now());
                last_arrival = Instant::now();
                sink.on_frame(to_report_frame(wire), aligned_ts);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                sink.idle_tick(Timestamp::now());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Ok(());
            }
        }

        if contacts_down && last_arrival.elapsed() > IDLE_LIFT_AFTER {
            // The client stopped sending while contacts were down. That
            // is a link fault, not a lift: a phone whose finger is
            // resting still has the finger on the glass, and any client
            // that only transmits on movement looks exactly like one
            // that walked away. Cancel the gesture instead of
            // completing it — completing it manufactures taps the user
            // never made.
            contacts_down = false;
            log::info!(
                "[net] no frames for {:?} with contacts down — canceling touch (link fault, not a lift)",
                IDLE_LIFT_AFTER
            );
            sink.on_link_timeout(Timestamp::now());
            scan_clock = ScanTimeClock::new();
        }
    }
}

// --- UDP producer -----------------------------------------------------------

/// Replayed-duplicate window. The only legitimate replay is the
/// client's immediate retransmit of a gesture-end ("all lifted") frame,
/// which lands milliseconds after the original. An identical seq
/// arriving *later* than this is a fresh session whose sender restarted
/// its counter (scripts do; phones will too after reboot) — it must be
/// admitted, not dropped.
const REPLAY_WINDOW: Duration = Duration::from_millis(600);
/// Warn about mid-stream loss only while traffic is flowing; a big seq
/// jump after a long idle period is just session churn.
const GAP_LOG_WINDOW: Duration = Duration::from_secs(2);
/// A backward jump this large is a sender restart rather than a packet
/// arriving late. The sender's UDP source port normally changes too, but
/// treating the jump explicitly keeps a reused port from staying stale.
const SEQ_RESTART_DISTANCE: i64 = 1_000_000;

/// State for one UDP sender. Sequence numbers are scoped to a sender, so
/// two phones (or a phone and the test sender) cannot suppress each other.
#[derive(Default)]
struct UdpFlow {
    seen: HashMap<u32, Instant>,
    last: Option<(u32, Instant)>,
}

/// Per-peer ingest filter for unordered, lossy UDP.
#[derive(Default)]
struct UdpIngest {
    flows: HashMap<SocketAddr, UdpFlow>,
}

#[derive(Debug, PartialEq, Eq)]
enum Admit {
    /// New seq (or an old one outside the replay window — treat alike).
    Fresh { lost_since_last: Option<u32> },
    /// Same seq seen inside [`REPLAY_WINDOW`] — safety copy; pass only
    /// if it's an all-lifted frame (handled by the caller).
    Replay,
}

impl UdpIngest {
    fn admit(&mut self, peer: SocketAddr, seq: u32) -> Admit {
        let flow = self.flows.entry(peer).or_default();
        let now = Instant::now();
        flow.seen
            .retain(|_, seen_at| now.duration_since(*seen_at) < REPLAY_WINDOW);
        if let Some(seen_at) = flow.seen.get(&seq)
            && now.duration_since(*seen_at) < REPLAY_WINDOW
        {
            return Admit::Replay;
        }
        flow.seen.insert(seq, now);

        let mut lost_since_last = None;
        if let Some((last_seq, last_at)) = flow.last {
            if now.duration_since(last_at) < GAP_LOG_WINDOW {
                // Interpret the wrapping subtraction as a signed serial
                // distance. Casting directly to i64 would turn a packet
                // one step behind into +4_294_967_295 and report a bogus
                // multi-billion-frame gap.
                let dist = seq.wrapping_sub(last_seq) as i32 as i64;
                // Only a forward jump indicates loss. An unseen packet
                // arriving late must not move the high-water mark or make
                // a later in-order packet look like another gap.
                if dist > 1 {
                    lost_since_last = Some((dist - 1) as u32);
                }
                if !(-SEQ_RESTART_DISTANCE..=0).contains(&dist) {
                    flow.last = Some((seq, now));
                }
            } else {
                flow.last = Some((seq, now));
            }
        } else {
            flow.last = Some((seq, now));
        }
        Admit::Fresh { lost_since_last }
    }
}

fn udp_reader(
    socket: UdpSocket,
    tx: mpsc::Sender<Incoming>,
    stats: Arc<Stats>,
    token: Option<&str>,
) {
    let mut ingest = UdpIngest::default();
    // Max authenticated datagram = 6-byte envelope + 256-byte token +
    // 115-byte ATP1 frame. Keep headroom for future envelope versions.
    let mut buf = [0u8; 512];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                stats.udp_datagrams.fetch_add(1, Ordering::Relaxed);
                let payload = match authenticated_udp_payload(&buf[..n], token) {
                    Some(payload) => payload,
                    None => {
                        stats.decode_errors.fetch_add(1, Ordering::Relaxed);
                        log::debug!("[net] dropping UDP frame with invalid authentication");
                        continue;
                    }
                };
                let frame = match touchpad_proto::decode(payload) {
                    Ok(f) => f,
                    Err(DecodeError::TooShort) | Err(DecodeError::BadMagic) => {
                        stats.decode_errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    Err(e) => {
                        stats.decode_errors.fetch_add(1, Ordering::Relaxed);
                        log::debug!("[net] dropping malformed datagram ({e})");
                        continue;
                    }
                };
                match ingest.admit(peer, frame.seq) {
                    Admit::Fresh { lost_since_last } => {
                        if let Some(lost) = lost_since_last {
                            let _ = tx.send(Incoming::Gap { peer: PeerId { transport: PeerTransport::Udp, addr: peer }, lost });
                        }
                        let _ = tx.send(Incoming::Frame { peer: PeerId { transport: PeerTransport::Udp, addr: peer }, frame });
                    }
                    Admit::Replay if frame.contacts.is_empty() => {
                        // Retransmitted lift — intentional safety copy.
                        let _ = tx.send(Incoming::Frame { peer: PeerId { transport: PeerTransport::Udp, addr: peer }, frame });
                    }
                    Admit::Replay => {} // superseded motion frame
                }
            }
            Err(e) => {
                log::warn!("[net] udp recv error: {e}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn to_report_frame(f: touchpad_proto::Frame) -> Frame {
    Frame {
        contacts: f
            .contacts
            .iter()
            .map(|c| Contact {
                id: c.id,
                x: c.x as f64,
                y: c.y as f64,
                tip: c.tip,
                confidence: c.confidence,
            })
            .collect(),
        scan_time_100us: f.scan_time_u16(),
        button: f.button,
    }
}

// --- HTTP + WebSocket (browser touchpad) ------------------------------------

struct WebPageContext {
    html: String,
    tester_html: String,
    port: u16,
}

fn tcp_acceptor(
    listener: TcpListener,
    tx: mpsc::Sender<Incoming>,
    page: WebPageContext,
    stats: Arc<Stats>,
    token: Option<&str>,
) {
    for conn in listener.incoming() {
        let stream = match conn {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tx = tx.clone();
        let stats = Arc::clone(&stats);
        let page = WebPageContext {
            html: page.html.clone(),
            tester_html: page.tester_html.clone(),
            port: page.port,
        };
        let token = token.map(str::to_owned);
        std::thread::spawn(move || {
            let _ = handle_conn(stream, tx, page, stats, token.as_deref());
        });
    }
}

fn handle_conn(
    mut stream: TcpStream,
    tx: mpsc::Sender<Incoming>,
    page: WebPageContext,
    stats: Arc<Stats>,
    token: Option<&str>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let head = read_http_head(&mut stream)?;
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("/");
    let clean_path = raw_path.split('?').next().unwrap_or(raw_path).trim_end_matches('/');

    if method != "GET" {
        write_simple(&mut stream, 405, "method not allowed")?;
        return Ok(());
    }

    // WebSocket upgrade for the touchpad event channel.
    let wants_ws = (clean_path == "/ws" || raw_path == "/ws") && head.to_ascii_lowercase().contains("upgrade: websocket");
    if wants_ws {
        if !authorized_http_request(&head, raw_path, token) {
            write_unauthorized(&mut stream)?;
            return Ok(());
        }
        let peer = PeerId {
            transport: PeerTransport::WebSocket,
            addr: stream.peer_addr()?,
        };
        let key = ws_handshake_key(&head).ok_or_else(|| anyhow::anyhow!("no Sec-WebSocket-Key"))?;
        write!(
            stream,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {key}\r\n\r\n"
        )?;
        stream.flush()?;
        ws_relay(stream, peer, tx, &stats)
    } else {
        let lower_path = clean_path.to_ascii_lowercase();
        if lower_path.is_empty() || lower_path == "/index.html" || lower_path == "/touchpad.html" || lower_path == "/touchpad" {
            write_simple(&mut stream, 200, &page.html)?;
            Ok(())
        } else if lower_path == "/test" || lower_path == "/tester" || lower_path == "/test.html" || lower_path == "/tester.html" {
            write_simple(&mut stream, 200, &page.tester_html)?;
            Ok(())
        } else {
        let help_body = format!(
            "<!DOCTYPE html><html><body style='background:#0b0c10;color:#eee;font-family:sans-serif;padding:40px;'>\
            <h2>404 Not Found</h2>\
            <p>请求路径: <code>{}</code> 未找到。</p>\
            <p>请点击直达页面：</p>\
            <ul>\
              <li><a href='/' style='color:#00e5ff;'>📱 手机触控板主页 (/)</a></li>\
              <li><a href='/test' style='color:#00e5ff;'>🛠 触控板全功能诊断套件 (/test)</a></li>\
            </ul>\
            </body></html>",
            raw_path
        );
            write_simple(&mut stream, 404, &help_body)
        }
    }
}

fn read_http_head(stream: &mut TcpStream) -> Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            anyhow::bail!("connection closed before header end");
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 8192 {
            anyhow::bail!("http head too large");
        }
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest_start = pos + 4;
            return Ok(String::from_utf8_lossy(&buf[..rest_start]).into_owned());
        }
    }
}

fn write_simple(stream: &mut TcpStream, code: u16, body: &str) -> Result<()> {
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let ctype = if body.starts_with("<!DOCTYPE") || body.starts_with("<html") {
        "text/html; charset=utf-8"
    } else {
        "text/plain"
    };
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache, no-store, must-revalidate, max-age=0\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()?;
    Ok(())
}

fn write_unauthorized(stream: &mut TcpStream) -> Result<()> {
    let body = "unauthorized";
    write!(
        stream,
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()?;
    Ok(())
}

/// Return the ATP1 payload from a UDP datagram. With no configured token,
/// v1 packets remain byte-for-byte compatible. With a token, callers must use
/// `ATK1 | token_len:u16 | token | ATP1 frame`.
fn authenticated_udp_payload<'a>(buf: &'a [u8], token: Option<&str>) -> Option<&'a [u8]> {
    let Some(expected) = token else {
        return Some(buf);
    };
    if buf.len() < AUTH_HEADER_LEN || buf[..4] != AUTH_MAGIC {
        return None;
    }
    let token_len = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    if token_len == 0 || token_len > AUTH_TOKEN_MAX_BYTES {
        return None;
    }
    let payload_at = AUTH_HEADER_LEN.checked_add(token_len)?;
    if payload_at >= buf.len() {
        return None;
    }
    if !constant_time_eq(&buf[AUTH_HEADER_LEN..payload_at], expected.as_bytes()) {
        return None;
    }
    Some(&buf[payload_at..])
}

fn authorized_http_request(head: &str, raw_path: &str, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let header_token = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("authorization") {
            return None;
        }
        let value = value.trim();
        value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::to_owned)
    });
    if header_token
        .as_deref()
        .is_some_and(|candidate| constant_time_eq(candidate.as_bytes(), expected.as_bytes()))
    {
        return true;
    }
    query_param(raw_path, "token")
        .is_some_and(|candidate| constant_time_eq(candidate.as_bytes(), expected.as_bytes()))
}

fn query_param(path: &str, wanted: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if name != wanted {
            return None;
        }
        percent_decode(value)
    })
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16)? as u8;
                let lo = (bytes[i + 2] as char).to_digit(16)? as u8;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// RFC 6455 Sec-WebSocket-Accept = base64(SHA1(key + GUID)).
fn ws_handshake_key(head: &str) -> Option<String> {
    const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    use sha1::{Digest, Sha1};
    let key_line = head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("sec-websocket-key:"))?;
    let key = key_line.split_once(':')?.1.trim();
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(GUID.as_bytes());
    Some(base64(&hasher.finalize()))
}

fn base64(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let word = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TBL[(word >> 18) as usize & 63] as char);
        out.push(TBL[(word >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TBL[(word >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TBL[word as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Relays WebSocket binary frames into the pump channel until the peer
/// closes. Only unfragmented binary payloads ≤ 255 B are accepted —
/// the protocol's largest legal frame is 115 B.
fn ws_relay(
    mut stream: TcpStream,
    peer: PeerId,
    tx: mpsc::Sender<Incoming>,
    stats: &Stats,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(300)))?;
    // TCP delivers everything in order — no dedup needed at all.
    let mut header = [0u8; 2];

    loop {
        read_exact(&mut stream, &mut header)?;
        let fin = header[0] & 0x80 != 0;
        let opcode = header[0] & 0x0F;
        let masked = header[1] & 0x80 != 0;
        let mut len = (header[1] & 0x7F) as usize;

        if len == 126 {
            let mut ext = [0u8; 2];
            read_exact(&mut stream, &mut ext)?;
            len = u16::from_be_bytes(ext) as usize;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            read_exact(&mut stream, &mut ext)?;
            let extended_len = u64::from_be_bytes(ext);
            if extended_len > 4096 {
                anyhow::bail!("ws frame too large (len={extended_len})");
            }
            len = extended_len as usize;
        }
        if len > 4096 || !masked || !fin {
            anyhow::bail!("ws frame unsupported (len={len} masked={masked} fin={fin})");
        }

        let mask_key = {
            let mut k = [0u8; 4];
            read_exact(&mut stream, &mut k)?;
            k
        };
        let mut payload = vec![0u8; len];
        read_exact(&mut stream, &mut payload)?;
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask_key[i % 4];
        }

        match opcode {
            0x8 => return Ok(()), // close
            0x9 => {
                // ping → pong (empty, masked not needed server-side)
                let _ = stream.write_all(&[0x8A, 0x00]);
                let _ = stream.flush();
            }
            0x1 => { // text — probe/status pings; ignore
            }
            0x2 => match touchpad_proto::decode(&payload) {
                Ok(f) => {
                    stats.ws_frames.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.send(Incoming::Frame { peer, frame: f });
                }
                Err(e) => log::debug!("[net] bad ws frame: {e}"),
            },
            _ => {}
        }
    }
}

fn read_exact(stream: &mut TcpStream, buf: &mut [u8]) -> Result<()> {
    stream.read_exact(buf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(port: u16) -> SocketAddr {
        ([127, 0, 0, 1], port).into()
    }

    fn endpoint(transport: PeerTransport, port: u16) -> PeerId {
        PeerId {
            transport,
            addr: peer(port),
        }
    }

    #[test]
    fn websocket_accept_matches_rfc6455_vector() {
        let request = "GET /chat HTTP/1.1\r\n".to_owned()
            + "Host: server.example.com\r\n"
            + "Upgrade: websocket\r\n"
            + "Connection: Upgrade\r\n"
            + "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n";
        assert_eq!(
            ws_handshake_key(&request).as_deref(),
            Some("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
    }

    #[test]
    fn udp_sequences_are_scoped_to_peer_and_reordering_is_not_a_gap() {
        let mut ingest = UdpIngest::default();
        let a = peer(1001);
        let b = peer(1002);

        assert_eq!(
            ingest.admit(a, 10),
            Admit::Fresh {
                lost_since_last: None
            }
        );
        // A second sender starts at an unrelated sequence number.
        assert_eq!(
            ingest.admit(b, 900_000),
            Admit::Fresh {
                lost_since_last: None
            }
        );
        assert_eq!(
            ingest.admit(a, 12),
            Admit::Fresh {
                lost_since_last: Some(1)
            }
        );
        // A late packet is still accepted, but does not rewind the
        // high-water mark or create a false gap for seq 13.
        assert_eq!(
            ingest.admit(a, 11),
            Admit::Fresh {
                lost_since_last: None
            }
        );
        assert_eq!(
            ingest.admit(a, 13),
            Admit::Fresh {
                lost_since_last: None
            }
        );
        assert_eq!(ingest.admit(a, 13), Admit::Replay);
    }

    #[test]
    fn peer_switch_is_reported_once_and_old_source_is_quarantined() {
        let mut gate = PeerGate::default();
        let t0 = Instant::now();
        let udp = endpoint(PeerTransport::Udp, 1001);
        let ws = endpoint(PeerTransport::WebSocket, 1002);

        assert_eq!(
            gate.observe(udp, t0),
            PeerDecision::Accept { switched_from: None }
        );
        assert_eq!(
            gate.observe(udp, t0 + Duration::from_millis(10)),
            PeerDecision::Accept { switched_from: None }
        );
        assert_eq!(
            gate.observe(ws, t0 + Duration::from_millis(20)),
            PeerDecision::Accept {
                switched_from: Some(udp)
            }
        );
        assert_eq!(
            gate.observe(udp, t0 + Duration::from_millis(30)),
            PeerDecision::IgnoreRetired
        );
        assert_eq!(
            gate.observe(udp, t0 + PEER_QUARANTINE + Duration::from_millis(21)),
            PeerDecision::Accept {
                switched_from: Some(ws)
            }
        );
    }

    #[test]
    fn transport_is_part_of_peer_identity() {
        let mut gate = PeerGate::default();
        let t0 = Instant::now();
        let udp = endpoint(PeerTransport::Udp, 1001);
        let ws = endpoint(PeerTransport::WebSocket, 1001);
        assert_eq!(
            gate.observe(udp, t0),
            PeerDecision::Accept { switched_from: None }
        );
        assert_eq!(
            gate.observe(ws, t0 + Duration::from_millis(1)),
            PeerDecision::Accept {
                switched_from: Some(udp)
            }
        );
    }

    #[test]
    fn authenticated_udp_envelope_preserves_atp1_payload() {
        let frame = touchpad_proto::Frame {
            seq: 7,
            scan_time_100us: 123,
            button: false,
            contacts: Vec::new(),
        };
        let encoded = frame.encode();
        assert_eq!(authenticated_udp_payload(&encoded, None), Some(encoded.as_slice()));

        let token = b"s3cret";
        let mut envelope = Vec::from(AUTH_MAGIC);
        envelope.extend_from_slice(&(token.len() as u16).to_le_bytes());
        envelope.extend_from_slice(token);
        envelope.extend_from_slice(&encoded);
        assert_eq!(
            authenticated_udp_payload(&envelope, Some("s3cret")),
            Some(encoded.as_slice())
        );
        assert_eq!(authenticated_udp_payload(&envelope, Some("wrong")), None);
        assert_eq!(authenticated_udp_payload(&encoded, Some("s3cret")), None);
    }

    #[test]
    fn websocket_auth_accepts_bearer_header_or_encoded_query() {
        let header = "GET /ws HTTP/1.1\r\nAuthorization: Bearer s3cret\r\n\r\n";
        assert!(authorized_http_request(header, "/ws", Some("s3cret")));
        assert!(!authorized_http_request(header, "/ws", Some("wrong")));
        assert!(authorized_http_request(
            "GET /ws?token=s3cret HTTP/1.1\r\n\r\n",
            "/ws?token=s3cret",
            Some("s3cret")
        ));
        assert!(authorized_http_request(
            "GET /ws?token=s%33cret HTTP/1.1\r\n\r\n",
            "/ws?token=s%33cret",
            Some("s3cret")
        ));
        assert!(authorized_http_request("", "/ws", None));
    }
}
