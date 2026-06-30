//! Opt-in **process-log visibility** for the base's long-running commands.
//!
//! # The swallow this addresses
//!
//! A base (`claude` / `codex` / `opencode`) runs a long shell command — a Maven
//! / Gradle / `npm`/`cargo` build, a `spring-boot:run`, a dependency install —
//! inside its OWN agentic tool loop and its OWN sandbox. The base CAPTURES that
//! command's stdout/stderr internally and only hands UmaDev a single, structured
//! `tool_result` (codex: a `commandExecution` item's `aggregatedOutput`) when the
//! command FINISHES — and each driver then clips that preview to a tight
//! [`DEFAULT_TOOL_OUTPUT_CAP`] chars. So during a multi-minute build the user sees
//! a silent "thinking" with no log lines, and even at the end only a 200-char
//! clip — the logs feel "swallowed by the sandbox redirect."
//!
//! There is no raw byte stream of the running command available from the base's
//! wire protocol (the base owns the pipe), so UmaDev cannot tail the file the
//! sandbox redirects to. What IT can do, when the user opts in, is (1) surface the
//! FULL captured output instead of a 200-char clip, and (2) for the codex
//! app-server — whose item lifecycle DOES emit `item/started` + `item/updated`
//! frames as the command runs — surface the running command immediately and stream
//! its growing output, so the multi-minute void becomes a live, progressing log.
//!
//! # The toggle
//!
//! [`SHOW_PROCESS_LOGS_ENV`] (`UMADEV_SHOW_PROCESS_LOGS`) — truthy turns it on.
//! The TUI publishes it from the saved preference at startup and flips it live via
//! the `/logs` command, so the next turn/session picks it up. **OFF by default**:
//! every driver behaves exactly as before.
//!
//! Fail-open by contract: every function here is total and never panics — an unset
//! / unparsable env is simply "off", and the cap always returns a usable bound.

/// Env toggle. Truthy (`1` / `true` / `yes` / `on`, case-insensitive) makes the
/// base drivers surface the FULL long-running command output (and, for codex,
/// stream it as it runs) instead of a tight clip. Anything else (incl. unset) is
/// off — the historical behaviour.
pub const SHOW_PROCESS_LOGS_ENV: &str = "UMADEV_SHOW_PROCESS_LOGS";

/// Per-command output preview cap (chars) when process logs are OFF — the
/// historical tight clip that keeps a chatty tool result from flooding the
/// transcript.
const DEFAULT_TOOL_OUTPUT_CAP: usize = 200;

/// Per-command output cap (chars) when process logs are ON — generous enough to
/// carry a real build log's signal (the tail of an `mvn` / `gradle` run) while
/// still being a hard bound, so even verbose mode can't surface an unbounded blob.
const VERBOSE_TOOL_OUTPUT_CAP: usize = 16 * 1024;

/// `true` when the user opted in to seeing the base's long-running process logs.
/// Read fresh each call so a live `/logs` toggle takes effect on the next turn.
/// Fail-open: unset / unparsable → `false`.
#[must_use]
pub fn show_process_logs() -> bool {
    is_truthy(std::env::var(SHOW_PROCESS_LOGS_ENV).ok().as_deref())
}

/// Pure, testable core of [`show_process_logs`]: a lenient truthy check.
fn is_truthy(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Pure mapping from the on/off flag to the per-command output preview cap: the
/// generous [`VERBOSE_TOOL_OUTPUT_CAP`] when ON, else the tight
/// [`DEFAULT_TOOL_OUTPUT_CAP`]. Takes the flag explicitly so a caller that already
/// resolved [`show_process_logs`] once (e.g. a per-line dispatch) doesn't re-read
/// the env — and so it is unit-testable without mutating process env.
#[must_use]
pub fn cap_for(on: bool) -> usize {
    if on {
        VERBOSE_TOOL_OUTPUT_CAP
    } else {
        DEFAULT_TOOL_OUTPUT_CAP
    }
}

/// The per-command output preview cap the drivers truncate to, resolved from the
/// live [`show_process_logs`] toggle. Single source of truth so all three drivers
/// stay in lockstep.
#[must_use]
pub fn tool_output_cap() -> usize {
    cap_for(show_process_logs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_accepts_common_on_spellings_only() {
        for on in ["1", "true", "TRUE", " yes ", "On", "ON"] {
            assert!(is_truthy(Some(on)), "{on:?} should be truthy");
        }
        for off in [None, Some(""), Some("0"), Some("false"), Some("nope")] {
            assert!(!is_truthy(off), "{off:?} should be falsy");
        }
    }

    #[test]
    fn cap_is_tight_off_and_generous_on() {
        // Pure mapping from the boolean, independent of process env (which a
        // sibling test could be mutating in parallel).
        let off = cap_for(false);
        let on = cap_for(true);
        assert_eq!(off, DEFAULT_TOOL_OUTPUT_CAP);
        assert!(on >= off * 10, "verbose cap {on} >> tight cap {off}");
    }
}
