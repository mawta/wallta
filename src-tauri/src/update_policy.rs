//! Pure update policy, also runnable without the Tauri/Windows SDK dependency tree.
pub const MANIFEST_LIMIT: u64 = 256 * 1024;
pub const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
pub const DAY_MS: u64 = 24 * 60 * 60 * 1000;

pub fn stable_version(text: &str) -> Option<[u64; 3]> {
    let mut parts = text.split('+');
    let core = parts.next()?;
    if let Some(build) = parts.next() {
        if build
            .split('.')
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
            || parts.next().is_some()
        {
            return None;
        }
    }
    let mut numbers = core.split('.');
    let mut result = [0; 3];
    for number in &mut result {
        let s = numbers.next()?;
        if s.is_empty()
            || !s.bytes().all(|b| b.is_ascii_digit())
            || (s.len() > 1 && s.starts_with('0'))
        {
            return None;
        }
        *number = s.parse().ok()?;
    }
    numbers.next().is_none().then_some(result)
}
pub fn newer_stable(current: &str, remote: &str) -> bool {
    matches!((stable_version(current), stable_version(remote)), (Some(a), Some(b)) if b > a)
}
pub fn release_path(host: &str, path: &str, version: &str) -> bool {
    stable_version(version).is_some()
        && host == "github.com"
        && path
            == format!("/mawta/wallta/releases/download/v{version}/Wallta_{version}_x64-setup.exe")
}
pub fn redirect_path(host: &str, path: &str) -> bool {
    match host {
        "github.com" => {
            path.starts_with("/mawta/wallta/releases/download/")
                || path == "/mawta/wallta/releases/latest/download/latest.json"
        }
        "release-assets.githubusercontent.com"
        | "objects.githubusercontent.com"
        | "github-releases.githubusercontent.com" => {
            path.starts_with("/github-production-release-asset/")
        }
        _ => false,
    }
}
pub fn allow_attempt(last: Option<u64>, now: u64, automatic: bool) -> bool {
    !automatic || last.map_or(true, |previous| now.saturating_sub(previous) >= DAY_MS)
}
pub fn append_size(current: u64, chunk: usize, limit: u64) -> Result<u64, &'static str> {
    current
        .checked_add(chunk as u64)
        .filter(|n| *n <= limit)
        .ok_or("Update exceeds the allowed size.")
}

#[derive(Default)]
pub struct JobGate {
    pub generation: u64,
    pub running: bool,
}
impl JobGate {
    pub fn begin(&mut self) -> Option<u64> {
        if self.running {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.running = true;
        Some(self.generation)
    }
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn current(&self, generation: u64) -> bool {
        self.running && self.generation == generation
    }
    pub fn finish(&mut self) {
        self.running = false;
    }
}

#[derive(Default)]
pub struct MutationGate(std::sync::atomic::AtomicBool);
pub struct InstallBarrier<'a>(&'a MutationGate);
impl MutationGate {
    pub fn ensure_open(&self) -> Result<(), String> {
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            Err("Wallta is installing an update. Try again after restarting.".into())
        } else {
            Ok(())
        }
    }
    /// Caller holds the state mutex, so prior state persistence completes first.
    pub fn install(&self) -> Result<InstallBarrier<'_>, String> {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .map_err(|_| "An update is already installing.".to_string())?;
        Ok(InstallBarrier(self))
    }
}
impl Drop for InstallBarrier<'_> {
    fn drop(&mut self) {
        self.0 .0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_semver_rejects_prereleases_downgrades_and_malformed_versions() {
        assert!(newer_stable("0.1.2", "0.1.3"));
        assert!(newer_stable("0.1.2", "1.0.0+build.3"));
        for remote in [
            "0.1.2",
            "0.1.1",
            "1.0.0-beta.1",
            "v1.0.0",
            "01.2.3",
            "1.0",
            "1.2.3+",
            "1.2.3+a..b",
            "1.2.3+é",
        ] {
            assert!(!newer_stable("0.1.2", remote), "{remote}");
        }
        assert!(!newer_stable("1.2.3+old", "1.2.3+new"));
    }
    #[test]
    fn release_candidate_is_bound_to_exact_repository_version_and_nsis_name() {
        let valid = "/mawta/wallta/releases/download/v0.1.3/Wallta_0.1.3_x64-setup.exe";
        assert!(release_path("github.com", valid, "0.1.3"));
        assert!(!release_path("github.com.evil.test", valid, "0.1.3"));
        assert!(!release_path("github.com", valid, "0.1.4"));
        assert!(!release_path(
            "github.com",
            &valid.replace("mawta", "attacker"),
            "0.1.3"
        ));
        assert!(!release_path(
            "github.com",
            &valid.replace("setup.exe", "setup.exe/other"),
            "0.1.3"
        ));
    }
    #[test]
    fn redirects_allow_only_exact_https_release_hosts_and_repository_paths() {
        assert!(redirect_path(
            "release-assets.githubusercontent.com",
            "/github-production-release-asset/123/asset"
        ));
        assert!(redirect_path(
            "github.com",
            "/mawta/wallta/releases/download/v0.1.3/latest.json"
        ));
        for host in [
            "evil.test",
            "github.com.evil.test",
            "raw.githubusercontent.com",
        ] {
            assert!(!redirect_path(
                host,
                "/mawta/wallta/releases/download/v0.1.3/latest.json"
            ));
        }
        assert!(!redirect_path(
            "github.com",
            "/other/repo/releases/download/v1/file"
        ));
    }
    #[test]
    fn stream_limits_count_actual_chunks_with_overflow_protection() {
        assert_eq!(
            append_size(DOWNLOAD_LIMIT - 1, 1, DOWNLOAD_LIMIT),
            Ok(DOWNLOAD_LIMIT)
        );
        assert!(append_size(MANIFEST_LIMIT, 1, MANIFEST_LIMIT).is_err());
        assert!(append_size(u64::MAX, 1, u64::MAX).is_err());
    }
    #[test]
    fn daily_attempts_include_failed_attempts_and_handle_clock_rollback() {
        assert!(allow_attempt(None, 1, true));
        assert!(!allow_attempt(Some(10), 10 + DAY_MS - 1, true));
        assert!(allow_attempt(Some(10), 10 + DAY_MS, true));
        assert!(!allow_attempt(Some(100), 99, true));
        assert!(allow_attempt(Some(100), 99, false));
    }
    #[test]
    fn cancel_invalidates_worker_but_does_not_allow_overlapping_jobs() {
        let mut gate = JobGate::default();
        let old = gate.begin().unwrap();
        assert!(gate.current(old));
        assert!(gate.begin().is_none());
        gate.cancel();
        assert!(!gate.current(old));
        assert!(gate.begin().is_none());
        gate.finish();
        let new = gate.begin().unwrap();
        assert_ne!(old, new);
        assert!(gate.current(new));
    }
    #[test]
    fn install_barrier_blocks_new_mutations_and_reopens_after_failure() {
        let gate = MutationGate::default();
        assert!(gate.ensure_open().is_ok());
        let barrier = gate.install().unwrap();
        assert!(gate.ensure_open().is_err());
        assert!(gate.install().is_err());
        drop(barrier);
        assert!(gate.ensure_open().is_ok());
        assert!(gate.install().is_ok());
    }
}
