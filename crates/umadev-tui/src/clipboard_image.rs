//! Read an image directly from the local OS clipboard and materialise it as a
//! workspace file.
//!
//! A PTY only transports bytes, so an image-only clipboard never becomes a
//! bracketed-paste event. `Ctrl+V` is therefore an explicit TUI action which
//! runs the platform clipboard command off the render thread. No clipboard
//! crate is used: macOS and Windows have built-in commands; Linux uses the
//! conventional `wl-paste` / `xclip` tools and degrades honestly when absent.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum accepted clipboard image size. The complete PNG is removed when it
/// crosses this boundary; it is never silently truncated.
pub(crate) const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1);

const MACOS_SCRIPT: &str = r"on run argv
set targetFile to POSIX file (item 1 of argv)
set pngData to the clipboard as «class PNGf»
set fileRef to open for access targetFile with write permission
try
    set eof fileRef to 0
    write pngData to fileRef
    close access fileRef
on error errMsg number errNum
    try
        close access fileRef
    end try
    error errMsg number errNum
end try
end run";

const WINDOWS_SCRIPT: &str = "$img = [Windows.Forms.Clipboard]::GetImage(); \
if ($null -eq $img) { exit 1 }; \
$img.Save($args[0], [System.Drawing.Imaging.ImageFormat]::Png); \
$img.Dispose()";

/// Result delivered back to the UI loop after the blocking capture finishes.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CaptureResult {
    /// A validated PNG was written under `.umadev/pasted/`.
    Image(PathBuf),
    /// The clipboard did not expose a PNG. This is intentionally silent: a
    /// normal text paste follows the existing terminal `Event::Paste` path.
    NoImage,
    /// Linux clipboard integration is not installed.
    MissingTool(&'static str),
    /// A real image exceeded [`MAX_IMAGE_BYTES`] and was removed.
    TooLarge(u64),
    /// Directory creation, command execution, or validation failed unexpectedly.
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    /// The command itself writes the path passed in its argv.
    Direct,
    /// The command writes image bytes to stdout; Rust owns the destination file.
    Stdout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Platform {
    Macos,
    Windows,
    Wayland,
    X11,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandPlan {
    program: &'static str,
    args: Vec<OsString>,
    output: OutputMode,
    missing_hint: Option<&'static str>,
}

/// Cheap, pure preflight used before spawning the blocking worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Preflight {
    Ready,
    Remote,
    Tmux,
    Offline,
}

pub(crate) fn preflight(remote: bool, tmux: bool, offline: bool) -> Preflight {
    if remote {
        Preflight::Remote
    } else if tmux {
        Preflight::Tmux
    } else if offline {
        Preflight::Offline
    } else {
        Preflight::Ready
    }
}

/// Capture the current local clipboard image into the workspace.
///
/// This function blocks on a child process and must run on Tokio's blocking
/// pool. Every failure is represented as data; it never panics.
pub(crate) fn capture(project_root: &Path) -> CaptureResult {
    let Some(target) = next_target(project_root) else {
        return CaptureResult::Failed;
    };

    let platform = select_platform(
        std::env::consts::OS,
        is_wsl(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    );
    let plan = match platform {
        Platform::Macos => macos_plan(&target),
        Platform::Windows => {
            let Some(platform_path) = windows_target_path(&target) else {
                return CaptureResult::Failed;
            };
            windows_plan(&platform_path)
        }
        Platform::Wayland => linux_plan(true),
        Platform::X11 => linux_plan(false),
    };

    run_plan(&plan, &target)
}

/// Best-effort retention sweep. Only generated `.png` regular files older than
/// seven days are touched; directories, symlinks, and unrelated files survive.
pub(crate) fn cleanup_old(project_root: &Path) {
    let dir = pasted_dir(project_root);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_file() || path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let old = generated_png_expired(&path, true, meta.modified().ok(), now);
        if old {
            let _ = fs::remove_file(path);
        }
    }
}

fn generated_png_expired(
    path: &Path,
    is_file: bool,
    modified: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    is_file
        && path.extension().and_then(|e| e.to_str()) == Some("png")
        && modified
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > RETENTION)
}

fn select_platform(os: &str, wsl: bool, wayland: bool) -> Platform {
    match (os, wsl, wayland) {
        ("macos", _, _) => Platform::Macos,
        ("windows", _, _) | (_, true, _) => Platform::Windows,
        (_, false, true) => Platform::Wayland,
        _ => Platform::X11,
    }
}

fn pasted_dir(project_root: &Path) -> PathBuf {
    project_root.join(".umadev").join("pasted")
}

/// Generate the path exclusively from process/time/counter state. Clipboard
/// bytes never participate in path construction (the path-injection floor).
fn next_target(project_root: &Path) -> Option<PathBuf> {
    let dir = pasted_dir(project_root);
    if !safe_workspace_dir(project_root, &dir) {
        return None;
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let seq = NEXT_IMAGE.fetch_add(1, Ordering::Relaxed);
    Some(dir.join(format!("{millis}-{}-{seq}.png", std::process::id())))
}

/// Create the destination and prove its canonical parent remains under the
/// canonical workspace. Existing symlink components are rejected before and
/// after creation so `.umadev/pasted` cannot redirect the write elsewhere.
fn safe_workspace_dir(project_root: &Path, dir: &Path) -> bool {
    for component in [project_root.join(".umadev"), dir.to_path_buf()] {
        if fs::symlink_metadata(&component).is_ok_and(|m| m.file_type().is_symlink()) {
            return false;
        }
    }
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let (Ok(root), Ok(dir)) = (project_root.canonicalize(), dir.canonicalize()) else {
        return false;
    };
    dir.starts_with(root)
}

fn macos_plan(target: &Path) -> CommandPlan {
    CommandPlan {
        program: "osascript",
        args: vec![
            OsString::from("-e"),
            OsString::from(MACOS_SCRIPT),
            OsString::from("--"),
            target.as_os_str().to_owned(),
        ],
        output: OutputMode::Direct,
        missing_hint: None,
    }
}

fn windows_plan(target: &OsString) -> CommandPlan {
    CommandPlan {
        program: "powershell.exe",
        args: vec![
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            // Load System.Windows.Forms before touching Clipboard. `-STA` is a
            // process switch, not a script option; omitting it silently yields no image.
            OsString::from("-STA"),
            OsString::from("-Command"),
            OsString::from(format!(
                "Add-Type -AssemblyName System.Windows.Forms; {WINDOWS_SCRIPT}"
            )),
            target.clone(),
        ],
        output: OutputMode::Direct,
        missing_hint: None,
    }
}

fn linux_plan(wayland: bool) -> CommandPlan {
    if wayland {
        CommandPlan {
            program: "wl-paste",
            args: vec![OsString::from("--type"), OsString::from("image/png")],
            output: OutputMode::Stdout,
            missing_hint: Some("wl-clipboard"),
        }
    } else {
        CommandPlan {
            program: "xclip",
            args: ["-selection", "clipboard", "-t", "image/png", "-o"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            output: OutputMode::Stdout,
            missing_hint: Some("xclip"),
        }
    }
}

fn run_plan(plan: &CommandPlan, target: &Path) -> CaptureResult {
    // A generated name must never overwrite an existing user file.
    if target.exists() {
        return CaptureResult::Failed;
    }

    let mut command = Command::new(plan.program);
    command
        .args(&plan.args)
        .stdin(Stdio::null())
        .stderr(Stdio::null());

    let child = match plan.output {
        OutputMode::Direct => command.stdout(Stdio::null()).spawn(),
        OutputMode::Stdout => {
            let Ok(file) = OpenOptions::new().write(true).create_new(true).open(target) else {
                return CaptureResult::Failed;
            };
            command.stdout(Stdio::from(file)).spawn()
        }
    };

    let mut child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let _ = fs::remove_file(target);
            return plan
                .missing_hint
                .map_or(CaptureResult::Failed, CaptureResult::MissingTool);
        }
        Err(_) => {
            let _ = fs::remove_file(target);
            return CaptureResult::Failed;
        }
    };
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < CAPTURE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let Some(status) = status else {
        let _ = fs::remove_file(target);
        return CaptureResult::Failed;
    };
    if !status.success() {
        let _ = fs::remove_file(target);
        return CaptureResult::NoImage;
    }
    finish_capture(target)
}

fn finish_capture(target: &Path) -> CaptureResult {
    let Ok(meta) = fs::metadata(target) else {
        return CaptureResult::Failed;
    };
    let len = meta.len();
    if len > MAX_IMAGE_BYTES {
        let _ = fs::remove_file(target);
        return CaptureResult::TooLarge(len);
    }
    let mut signature = [0_u8; PNG_SIGNATURE.len()];
    let valid = File::open(target)
        .and_then(|mut file| file.read_exact(&mut signature))
        .is_ok()
        && signature == PNG_SIGNATURE;
    if !valid {
        let _ = fs::remove_file(target);
        return CaptureResult::NoImage;
    }
    CaptureResult::Image(target.to_path_buf())
}

fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || fs::read_to_string("/proc/sys/kernel/osrelease")
            .is_ok_and(|s| s.to_ascii_lowercase().contains("microsoft"))
}

fn windows_target_path(target: &Path) -> Option<OsString> {
    if cfg!(windows) {
        return Some(target.as_os_str().to_owned());
    }
    // WSL's PowerShell is a Windows process and cannot open a Linux `/home/...`
    // path. `wslpath` is part of WSL and performs the boundary conversion.
    let output = Command::new("wslpath")
        .args(["-w", "--"])
        .arg(target)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    Some(OsString::from(path.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn windows_argv_locks_sta_and_never_interpolates_the_path_into_script() {
        let path = OsString::from(r"C:\Users\我\shot path.png");
        let plan = windows_plan(&path);
        let args = strings(&plan.args);
        assert_eq!(plan.program, "powershell.exe");
        assert_eq!(
            &args[..4],
            ["-NoProfile", "-NonInteractive", "-STA", "-Command"]
        );
        assert!(args[4].contains("System.Windows.Forms"));
        assert!(args[4].contains("$args[0]"));
        assert!(!args[4].contains("shot path.png"));
        assert_eq!(args[5], r"C:\Users\我\shot path.png");
    }

    #[test]
    fn macos_argv_passes_the_generated_path_as_data() {
        let target = Path::new("/tmp/a path/图.png");
        let plan = macos_plan(target);
        let args = strings(&plan.args);
        assert_eq!(plan.program, "osascript");
        assert_eq!(args[0], "-e");
        assert!(args[1].contains("the clipboard as «class PNGf»"));
        assert!(args[1].contains("item 1 of argv"));
        assert!(!args[1].contains("/tmp/a path"));
        assert_eq!(&args[2..], ["--", "/tmp/a path/图.png"]);
    }

    #[test]
    fn linux_argv_is_exact_for_wayland_and_x11() {
        let wayland = linux_plan(true);
        assert_eq!(wayland.program, "wl-paste");
        assert_eq!(strings(&wayland.args), ["--type", "image/png"]);
        assert_eq!(wayland.missing_hint, Some("wl-clipboard"));

        let x11 = linux_plan(false);
        assert_eq!(x11.program, "xclip");
        assert_eq!(
            strings(&x11.args),
            ["-selection", "clipboard", "-t", "image/png", "-o"]
        );
        assert_eq!(x11.missing_hint, Some("xclip"));
    }

    #[test]
    fn platform_selection_covers_native_and_wsl_desktops() {
        assert_eq!(select_platform("macos", false, false), Platform::Macos);
        assert_eq!(select_platform("windows", false, false), Platform::Windows);
        assert_eq!(select_platform("linux", true, false), Platform::Windows);
        assert_eq!(select_platform("linux", false, true), Platform::Wayland);
        assert_eq!(select_platform("linux", false, false), Platform::X11);
    }

    #[test]
    fn preflight_is_honest_and_remote_wins_over_everything() {
        assert_eq!(preflight(false, false, false), Preflight::Ready);
        assert_eq!(preflight(true, false, false), Preflight::Remote);
        assert_eq!(preflight(false, true, false), Preflight::Tmux);
        assert_eq!(preflight(false, false, true), Preflight::Offline);
        assert_eq!(preflight(true, true, true), Preflight::Remote);
    }

    #[test]
    fn generated_targets_are_inside_workspace_and_ignore_hostile_clipboard_text() {
        let root = tempfile::tempdir().unwrap();
        let a = next_target(root.path()).unwrap();
        let b = next_target(root.path()).unwrap();
        assert!(a.starts_with(root.path().join(".umadev/pasted")));
        assert!(b.starts_with(root.path().join(".umadev/pasted")));
        assert_ne!(a, b);
        let name = a.file_name().unwrap().to_string_lossy();
        assert!(!name.contains(".."));
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
    }

    #[test]
    fn symlinked_destination_is_rejected() {
        #[cfg(unix)]
        {
            let root = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join(".umadev")).unwrap();
            std::os::unix::fs::symlink(outside.path(), root.path().join(".umadev/pasted")).unwrap();
            assert!(next_target(root.path()).is_none());
        }
    }

    #[test]
    fn valid_png_finishes_and_oversize_or_invalid_files_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let valid = root.path().join("valid.png");
        fs::write(&valid, [PNG_SIGNATURE.as_slice(), b"body"].concat()).unwrap();
        assert_eq!(finish_capture(&valid), CaptureResult::Image(valid.clone()));

        let invalid = root.path().join("invalid.png");
        fs::write(&invalid, b"not a png").unwrap();
        assert_eq!(finish_capture(&invalid), CaptureResult::NoImage);
        assert!(!invalid.exists());

        let large = root.path().join("large.png");
        let file = File::create(&large).unwrap();
        file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        assert_eq!(
            finish_capture(&large),
            CaptureResult::TooLarge(MAX_IMAGE_BYTES + 1)
        );
        assert!(!large.exists());
    }

    #[test]
    fn cleanup_only_removes_old_generated_png_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = pasted_dir(root.path());
        fs::create_dir_all(&dir).unwrap();
        let fresh = dir.join("fresh.png");
        let unrelated = dir.join("keep.txt");
        fs::write(&fresh, PNG_SIGNATURE).unwrap();
        fs::write(&unrelated, b"keep").unwrap();
        cleanup_old(root.path());
        assert!(fresh.exists());
        assert!(unrelated.exists());

        let now = UNIX_EPOCH + Duration::from_secs(20 * 24 * 60 * 60);
        let old = now - Duration::from_secs(8 * 24 * 60 * 60);
        assert!(generated_png_expired(
            Path::new("old.png"),
            true,
            Some(old),
            now
        ));
        assert!(!generated_png_expired(
            Path::new("old.txt"),
            true,
            Some(old),
            now
        ));
        assert!(!generated_png_expired(
            Path::new("old.png"),
            false,
            Some(old),
            now
        ));
    }
}
