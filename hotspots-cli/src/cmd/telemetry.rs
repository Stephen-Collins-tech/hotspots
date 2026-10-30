//! `hotspots telemetry` — opt-in, anonymous usage pings. Off unless the user enables it.
//! See docs/TELEMETRY.md for the exact payload and every way to turn it off.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://hotspots.dev/api/event";
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1000);
const PING_INTERVAL_SECS: u64 = 24 * 60 * 60;
const STATE_FILE: &str = "telemetry.json";
const CI_VARS: [&str; 5] = [
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BUILDKITE",
    "JENKINS_URL",
];

/// The complete list of what a ping contains, as `(field, meaning)`. This is the single source of
/// truth: `hotspots telemetry show`, the opt-in prompt, docs/TELEMETRY.md and a test all follow it.
/// Adding a field to `Meta` without listing it here fails `payload_has_only_the_documented_fields`.
pub(crate) const COLLECTED: [(&str, &str); 6] = [
    ("v", "hotspots version"),
    ("os", "operating system (e.g. linux, macos, windows)"),
    ("arch", "CPU architecture (e.g. x86_64, aarch64)"),
    (
        "channel",
        "install channel guessed from the executable path: npm, pypi, cargo, brew or binary",
    ),
    (
        "cmd",
        "name of the subcommand that ran, never its arguments",
    ),
    (
        "iid",
        "random install id, generated when you enable telemetry",
    ),
];

/// What the receiving server adds on its own. Nothing else is stored.
pub(crate) const SERVER_SIDE: [&str; 3] = [
    "the time the request arrived",
    "the request's country, from the CDN (your IP address is not stored)",
    "the User-Agent header, always the fixed string `hotspots-cli`",
];

/// Never collected, and not representable in the payload.
pub(crate) const NEVER_COLLECTED: &str = "source code, file or repo names, paths, languages, \
scores, analysis results, command arguments, environment variables, hostnames, usernames, IP addresses";

#[derive(clap::Subcommand)]
pub(crate) enum TelemetryAction {
    /// Show whether telemetry is enabled
    Status,
    /// Turn anonymous usage pings on
    Enable,
    /// Turn anonymous usage pings off
    Disable,
    /// Print the exact payload that would be sent, without sending it
    Show,
    /// Generate a new random install id
    Reset,
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    enabled: bool,
    /// The one-time prompt has been shown (whatever the answer was).
    asked: bool,
    install_id: String,
    last_ping_unix: u64,
}

#[derive(Serialize)]
struct Event {
    name: &'static str,
    meta: Meta,
}

/// Every field sent. Nothing derived from code, paths, repos or results.
#[derive(Serialize)]
struct Meta {
    v: String,
    os: &'static str,
    arch: &'static str,
    channel: &'static str,
    cmd: &'static str,
    iid: String,
}

fn state_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".hotspots").join(STATE_FILE))
}

fn read_state(path: &PathBuf) -> State {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_state(path: &PathBuf, state: &State) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(state)?)?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Random 128-bit hex id, not derived from the machine, user or any path. `RandomState`
/// is seeded from the OS per instance, so two hashers give two independent 64-bit halves.
fn new_install_id() -> String {
    let half = || RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", half(), half())
}

fn channel(exe_path: &str) -> &'static str {
    if exe_path.contains("/.cargo/") {
        "cargo"
    } else if exe_path.contains("node_modules") || exe_path.contains(".nvm") {
        "npm"
    } else if exe_path.contains("site-packages") || exe_path.contains("/.venv/") {
        "pypi"
    } else if exe_path.contains("Cellar") || exe_path.contains("homebrew") {
        "brew"
    } else {
        "binary"
    }
}

/// Environment-level opt-outs that win over a saved "enabled".
fn env_opted_out(get: &dyn Fn(&str) -> Option<String>) -> bool {
    let set = |k: &str| get(k).is_some_and(|v| !v.is_empty() && v != "0");
    set("DO_NOT_TRACK")
        || get("HOTSPOTS_TELEMETRY").is_some_and(|v| v == "0")
        || CI_VARS.iter().any(|k| set(k))
}

fn env_get(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

fn build_event(state: &State, cmd: &'static str) -> Event {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Event {
        name: "cli_ping",
        meta: Meta {
            v: env!("HOTSPOTS_VERSION").to_string(),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            channel: channel(&exe),
            cmd,
            iid: state.install_id.clone(),
        },
    }
}

fn send(event: &Event) {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let _ = agent
        .post(ENDPOINT)
        .header("User-Agent", "hotspots-cli")
        .send_json(event);
}

/// Called after a command finishes. Silent and best-effort: never changes exit codes or output.
pub(crate) fn after_command(cmd: &'static str) {
    if env_opted_out(&env_get) {
        return;
    }
    let Some(path) = state_path() else { return };
    let mut state = read_state(&path);

    if !state.asked && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        state.asked = true;
        state.enabled = prompt_opt_in();
        if state.enabled && state.install_id.is_empty() {
            state.install_id = new_install_id();
        }
        let _ = write_state(&path, &state);
    }

    if !state.enabled || state.install_id.is_empty() {
        return;
    }
    let now = now_unix();
    if now.saturating_sub(state.last_ping_unix) < PING_INTERVAL_SECS {
        return;
    }
    // Stamp before sending so a failing network doesn't retry on every run.
    state.last_ping_unix = now;
    let _ = write_state(&path, &state);
    send(&build_event(&state, cmd));
}

fn prompt_opt_in() -> bool {
    eprintln!();
    eprintln!("Share anonymous usage stats? At most once a day, this is everything sent:");
    for (field, meaning) in COLLECTED {
        eprintln!("  {field:<8} {meaning}");
    }
    eprintln!("Never sent: {NEVER_COLLECTED}.");
    eprintln!("See the exact payload any time: `hotspots telemetry show`.");
    eprint!("Enable? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    let enabled = matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
    eprintln!(
        "{}",
        if enabled {
            "Telemetry enabled. Disable any time: `hotspots telemetry disable`."
        } else {
            "Telemetry stays off."
        }
    );
    enabled
}

pub(crate) fn handle_telemetry(action: TelemetryAction) -> anyhow::Result<()> {
    let path = state_path().ok_or_else(|| anyhow::anyhow!("could not find a home directory"))?;
    let mut state = read_state(&path);
    match action {
        TelemetryAction::Status => {
            let overridden = env_opted_out(&env_get);
            println!(
                "Telemetry is {}{}.",
                if state.enabled { "enabled" } else { "disabled" },
                if state.enabled && overridden {
                    " but overridden off by the environment (DO_NOT_TRACK, HOTSPOTS_TELEMETRY=0 or CI)"
                } else {
                    ""
                }
            );
            println!("State file: {}", path.display());
        }
        TelemetryAction::Enable => {
            state.enabled = true;
            state.asked = true;
            if state.install_id.is_empty() {
                state.install_id = new_install_id();
            }
            write_state(&path, &state)?;
            println!("Telemetry enabled. See exactly what is sent with `hotspots telemetry show`.");
        }
        TelemetryAction::Disable => {
            state.enabled = false;
            state.asked = true;
            write_state(&path, &state)?;
            println!("Telemetry disabled.");
        }
        TelemetryAction::Show => {
            if state.install_id.is_empty() {
                state.install_id = "<generated when enabled>".to_string();
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&build_event(&state, "analyze"))?
            );
            println!("POST {ENDPOINT} at most once every 24 hours, only while enabled.");
            println!("\nFields:");
            for (field, meaning) in COLLECTED {
                println!("  {field:<8} {meaning}");
            }
            println!("\nAdded by the server:");
            for item in SERVER_SIDE {
                println!("  - {item}");
            }
            println!("\nNever collected: {NEVER_COLLECTED}.");
        }
        TelemetryAction::Reset => {
            state.install_id = new_install_id();
            write_state(&path, &state)?;
            println!("Generated a new random install id.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn opt_out_env_vars() {
        assert!(!env_opted_out(&env(&[])));
        assert!(env_opted_out(&env(&[("DO_NOT_TRACK", "1")])));
        assert!(env_opted_out(&env(&[("HOTSPOTS_TELEMETRY", "0")])));
        assert!(env_opted_out(&env(&[("CI", "true")])));
        assert!(env_opted_out(&env(&[("GITHUB_ACTIONS", "true")])));
        assert!(!env_opted_out(&env(&[("DO_NOT_TRACK", "0")])));
    }

    #[test]
    fn install_ids_are_random_hex() {
        let (a, b) = (new_install_id(), new_install_id());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn channel_from_exe_path() {
        assert_eq!(channel("/home/u/.cargo/bin/hotspots"), "cargo");
        assert_eq!(channel("/usr/lib/node_modules/x/bin/hotspots"), "npm");
        assert_eq!(
            channel("/opt/homebrew/Cellar/hotspots/1/bin/hotspots"),
            "brew"
        );
        assert_eq!(channel("/usr/local/bin/hotspots"), "binary");
    }

    #[test]
    fn payload_has_only_the_documented_fields() {
        let state = State {
            install_id: "abc".into(),
            ..Default::default()
        };
        let json = serde_json::to_value(build_event(&state, "analyze")).unwrap();
        assert_eq!(json["name"], "cli_ping");
        let mut keys: Vec<_> = json["meta"].as_object().unwrap().keys().cloned().collect();
        keys.sort();
        let mut documented: Vec<_> = COLLECTED.iter().map(|(f, _)| f.to_string()).collect();
        documented.sort();
        assert_eq!(keys, documented);
    }

    #[test]
    fn docs_list_every_collected_field() {
        let doc = include_str!("../../../docs/TELEMETRY.md");
        for (field, _) in COLLECTED {
            assert!(
                doc.contains(&format!("| `{field}`")),
                "docs missing {field}"
            );
        }
    }
}
