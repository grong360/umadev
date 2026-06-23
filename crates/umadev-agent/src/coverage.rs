//! Deterministic requirement-coverage check — the "real enforcement" that
//! Spec-Driven-Development research flags as the thing that actually matters: a
//! spec DOCUMENT does not guarantee the implementation (or even the task list)
//! covers it. After the spec phase, cross-check that every functional
//! requirement (`FR-NNN`) declared in the PRD is referenced by at least one
//! task, and surface the orphans so a requirement can't be silently dropped.
//!
//! This is the spec→tasks half of the verification loop; the architecture API
//! contract (`umadev-contract`) is the spec→code half. Pure + fail-open: any IO
//! error yields "nothing uncovered" so the check never blocks the pipeline.

use std::collections::BTreeSet;
use std::path::Path;

/// Functional-requirement ids (`FR-NNN`) the PRD declares but NO task cites —
/// i.e. requirements at risk of being silently dropped. Empty when everything is
/// covered, the PRD has no `FR-` ids, or the files can't be read.
#[must_use]
pub fn uncovered_requirements(project_root: &Path, slug: &str) -> Vec<String> {
    let prd = read(project_root.join("output").join(format!("{slug}-prd.md")));
    let declared = extract_fr_ids(&prd);
    if declared.is_empty() {
        return Vec::new();
    }
    // A requirement is "covered" if the execution plan OR any task list cites it.
    let mut cited = extract_fr_ids(&read(
        project_root
            .join("output")
            .join(format!("{slug}-execution-plan.md")),
    ));
    if let Some(tasks) = latest_tasks(project_root) {
        cited.extend(extract_fr_ids(&tasks));
    }
    declared.difference(&cited).cloned().collect()
}

fn read(path: std::path::PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The most-recent `.umadev/changes/<id>/tasks.md`.
///
/// P1-6: change ids are usually timestamp-suffixed, but a hand-named change like
/// `demo-hotfix` sorts lexicographically AFTER any digit-prefixed id, so a plain
/// `dirs.sort()` would pick the WRONG (non-newest) directory and read a stale
/// tasks list — silently misreporting coverage. Pick the newest by filesystem
/// mtime instead, falling back to the directory NAME only when mtime is
/// unavailable (so a deterministic order is still chosen, fail-open).
fn latest_tasks(project_root: &Path) -> Option<String> {
    let dir = project_root.join(".umadev").join("changes");
    let mut dirs: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Sort by (mtime, name): mtime is the real recency signal; the name is a
    // stable tiebreaker so two dirs with the same/unknown mtime still order
    // deterministically. A missing mtime sorts oldest (UNIX_EPOCH).
    dirs.sort_by_cached_key(|p| {
        let mtime = std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        (mtime, p.file_name().map(std::ffi::OsString::from))
    });
    std::fs::read_to_string(dirs.last()?.join("tasks.md")).ok()
}

/// Scan for `FR-<digits>` tokens (case-insensitive on `FR`), normalised to a
/// canonical zero-padded `FR-NNN`. `FR-` and ASCII digits are single-byte, so
/// byte indexing here is multibyte-safe even amid CJK prose.
///
/// P1-7: the digit run is PARSED to a number and re-formatted zero-padded, so
/// `FR-1`, `FR-01`, and `FR-001` all canonicalise to the SAME `FR-001`. Without
/// this, a PRD that writes `FR-001` and a tasks file that writes `FR-1` looked
/// like DIFFERENT requirements and produced a phantom "uncovered" report. A run
/// of digits that overflows `u32` (absurd, but fail-open) keeps the raw digits.
fn extract_fr_ids(text: &str) -> BTreeSet<String> {
    let b = text.as_bytes();
    let n = b.len();
    let mut ids = BTreeSet::new();
    let mut i = 0;
    while i + 3 < n {
        let is_fr = (b[i] | 0x20) == b'f' && (b[i + 1] | 0x20) == b'r' && b[i + 2] == b'-';
        if is_fr {
            let mut j = i + 3;
            while j < n && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 3 {
                ids.insert(normalize_fr(&text[i + 3..j]));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    ids
}

/// Canonicalise a run of FR digits to zero-padded `FR-NNN` so `1` / `01` / `001`
/// compare equal. Falls back to the raw digits (still prefixed) if the number
/// can't be parsed (e.g. it overflows `u32`) — fail-open, never panics.
fn normalize_fr(digits: &str) -> String {
    match digits.parse::<u32>() {
        Ok(num) => format!("FR-{num:03}"),
        Err(_) => format!("FR-{digits}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_diffs_fr_ids() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("output")).unwrap();
        std::fs::write(
            root.join("output").join("demo-prd.md"),
            "| FR-001 | 登录 | WHEN ... SHALL ... |\n| fr-002 | 登出 |\n| FR-003 | 注册 |",
        )
        .unwrap();
        let cdir = root.join(".umadev").join("changes").join("demo-20260101");
        std::fs::create_dir_all(&cdir).unwrap();
        // Tasks cover FR-001 and FR-002 (lowercase), but NOT FR-003.
        std::fs::write(
            cdir.join("tasks.md"),
            "- [ ] 实现登录 _(FR-001)_\n- [ ] 登出 _(fr-002)_",
        )
        .unwrap();
        let uncovered = uncovered_requirements(root, "demo");
        assert_eq!(uncovered, vec!["FR-003".to_string()]);
    }

    #[test]
    fn no_prd_requirements_means_nothing_uncovered() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(uncovered_requirements(tmp.path(), "demo").is_empty());
    }

    #[test]
    fn fr_ids_normalize_so_fr_1_equals_fr_001() {
        // P1-7: a PRD that writes FR-001 and a tasks file that writes FR-1 (or
        // FR-01) must be treated as the SAME requirement — no phantom "uncovered".
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("output")).unwrap();
        std::fs::write(
            root.join("output").join("demo-prd.md"),
            "| FR-001 | 登录 |\n| FR-002 | 登出 |\n| FR-010 | 注册 |",
        )
        .unwrap();
        let cdir = root.join(".umadev").join("changes").join("demo-20260101");
        std::fs::create_dir_all(&cdir).unwrap();
        // Tasks cite the SAME requirements with un-padded ids: FR-1, FR-2, FR-10.
        std::fs::write(
            cdir.join("tasks.md"),
            "- [ ] 登录 _(FR-1)_\n- [ ] 登出 _(FR-2)_\n- [ ] 注册 _(FR-10)_",
        )
        .unwrap();
        assert!(
            uncovered_requirements(root, "demo").is_empty(),
            "FR-1/FR-2/FR-10 must cover FR-001/FR-002/FR-010"
        );
    }

    #[test]
    fn normalize_fr_canonicalises_padding() {
        assert_eq!(normalize_fr("1"), "FR-001");
        assert_eq!(normalize_fr("01"), "FR-001");
        assert_eq!(normalize_fr("001"), "FR-001");
        assert_eq!(normalize_fr("42"), "FR-042");
        assert_eq!(normalize_fr("1000"), "FR-1000"); // 4-digit keeps its width
    }

    #[test]
    fn latest_tasks_picks_newest_by_mtime_not_lexicographic() {
        // P1-6: a hand-named change `demo-hotfix` sorts lexicographically AFTER a
        // timestamped `demo-20260101`, so a naive name sort would read the OLDER
        // dir. The NEWER dir (by mtime) must win regardless of name.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("output")).unwrap();
        std::fs::write(
            root.join("output").join("demo-prd.md"),
            "| FR-001 | login |\n| FR-002 | logout |",
        )
        .unwrap();
        let changes = root.join(".umadev").join("changes");
        // The OLDER, lexicographically-LARGER dir covers only FR-001.
        let older = changes.join("demo-hotfix");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::write(older.join("tasks.md"), "- [ ] login _(FR-001)_").unwrap();
        // Make the timestamped dir clearly NEWER by mtime, even though its name
        // sorts BEFORE `demo-hotfix`. It covers BOTH requirements.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let newer = changes.join("demo-20260101");
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(
            newer.join("tasks.md"),
            "- [ ] login _(FR-001)_\n- [ ] logout _(FR-002)_",
        )
        .unwrap();
        // If latest_tasks picked the newer (correct) dir, BOTH are covered →
        // nothing uncovered. If it wrongly picked `demo-hotfix`, FR-002 leaks.
        assert!(
            uncovered_requirements(root, "demo").is_empty(),
            "the newest-by-mtime tasks dir must be the one consulted"
        );
    }
}
