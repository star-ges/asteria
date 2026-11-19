//! Moving Omacraft's data over on Asteria's first start.
//!
//! Asteria continues Omacraft, so its instances, worlds, accounts, backups and settings
//! are in `~/.local/share/omacraft` and `~/.config/omacraft`. Each is moved by a rename
//! (same filesystem: instant, reflinks and permissions kept, `accounts.json` stays 0600),
//! and a relative link `omacraft → asteria` is left in its place. The link keeps working
//! what still names the old path: absolute share links, the sources recorded in backups,
//! and an Omacraft that is still installed, which then uses the same files (the two
//! exclude each other through the same `flock`s).
//!
//! Nothing moves while Omacraft runs (or holds the shared folders' lock): the move would
//! pull its files from under it. Asteria then waits without creating its own folders, or
//! a half-made `asteria` folder would stop the move for good.

use std::path::Path;

use crate::{fsutil, paths};

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing of Omacraft's to move, or it's been moved already.
    Nothing,
    /// The data and/or config folder was moved.
    Moved,
    /// Omacraft is running: try again once it's quit.
    OmacraftRunning,
}

/// Move Omacraft's folders if they're there and Asteria's aren't.
pub fn run() -> anyhow::Result<Outcome> {
    let (Some(data), Some(config)) = (dirs::data_dir(), dirs::config_dir()) else {
        return Ok(Outcome::Nothing);
    };
    run_in(&data, &config, omacraft_running)
}

fn omacraft_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", paths::LEGACY_APP_ID])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Whether `base/omacraft` is a real folder waiting to move (not the link a move leaves),
/// and `base/asteria` doesn't exist yet.
fn pending(base: &Path) -> bool {
    let old = base.join(paths::LEGACY_APP_ID);
    let new = base.join(paths::APP_ID);
    std::fs::symlink_metadata(&new).is_err() && std::fs::symlink_metadata(&old).is_ok_and(|m| m.is_dir())
}

fn run_in(data_base: &Path, config_base: &Path, running: impl Fn() -> bool) -> anyhow::Result<Outcome> {
    let bases = [data_base, config_base];
    if !bases.iter().any(|b| pending(b)) {
        return Ok(Outcome::Nothing);
    }
    if running() {
        return Ok(Outcome::OmacraftRunning);
    }
    // A job of another process (an Omacraft CLI command, say) holding the shared folders.
    let shared_lock = data_base.join(paths::LEGACY_APP_ID).join("shared/.lock");
    let _held = if shared_lock.exists() {
        match fsutil::try_lock(&shared_lock)? {
            Some(l) => Some(l),
            None => return Ok(Outcome::OmacraftRunning),
        }
    } else {
        None
    };
    for base in bases {
        if !pending(base) {
            continue;
        }
        let old = base.join(paths::LEGACY_APP_ID);
        let new = base.join(paths::APP_ID);
        fsutil::rename_noreplace(&old, &new)
            .map_err(|e| anyhow::anyhow!("couldn't move {} to {}: {e}", old.display(), new.display()))?;
        std::os::unix::fs::symlink(paths::APP_ID, &old)
            .map_err(|e| anyhow::anyhow!("moved {} to {}, but couldn't leave a link behind: {e}", old.display(), new.display()))?;
    }
    Ok(Outcome::Moved)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn scratch(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = paths::test_root().join("migrate").join(name);
        let _ = std::fs::remove_dir_all(&root);
        let (data, config) = (root.join("share"), root.join("config"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        (data, config)
    }

    #[test]
    fn migration_moves_and_links_an_omacraft_dir() {
        let (data, config) = scratch("moves");
        let old = data.join("omacraft");
        std::fs::create_dir_all(old.join("instances/x/minecraft/saves/W")).unwrap();
        std::fs::write(old.join("accounts.json"), "{}").unwrap();
        std::fs::set_permissions(old.join("accounts.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::create_dir_all(config.join("omacraft")).unwrap();
        std::fs::write(config.join("omacraft/config.toml"), "backups_keep = 3\n").unwrap();

        assert_eq!(run_in(&data, &config, || false).unwrap(), Outcome::Moved);

        let new = data.join("asteria");
        assert!(new.join("instances/x/minecraft/saves/W").is_dir());
        let mode = std::fs::metadata(new.join("accounts.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_link(&old).unwrap(), Path::new("asteria"));
        // The old path still reaches the same files (absolute links, backup sources).
        assert!(old.join("instances/x/minecraft/saves/W").is_dir());
        assert_eq!(std::fs::read_to_string(config.join("asteria/config.toml")).unwrap(), "backups_keep = 3\n");
        assert!(std::fs::symlink_metadata(config.join("omacraft")).unwrap().is_symlink());

        // A second start finds nothing to do.
        assert_eq!(run_in(&data, &config, || false).unwrap(), Outcome::Nothing);
    }

    #[test]
    fn migration_waits_while_omacraft_runs() {
        let (data, config) = scratch("waits");
        std::fs::create_dir_all(data.join("omacraft/instances")).unwrap();
        assert_eq!(run_in(&data, &config, || true).unwrap(), Outcome::OmacraftRunning);
        assert!(data.join("omacraft").is_dir());
        assert!(!data.join("asteria").exists());
    }

    #[test]
    fn migration_waits_for_a_held_shared_lock() {
        let (data, config) = scratch("lock");
        std::fs::create_dir_all(data.join("omacraft/shared")).unwrap();
        let held = crate::fsutil::tests::FlockHolder::lock(&data.join("omacraft/shared/.lock"));
        assert_eq!(run_in(&data, &config, || false).unwrap(), Outcome::OmacraftRunning);
        drop(held);
        assert_eq!(run_in(&data, &config, || false).unwrap(), Outcome::Moved);
    }

    #[test]
    fn an_existing_asteria_dir_is_never_replaced() {
        let (data, config) = scratch("exists");
        std::fs::create_dir_all(data.join("omacraft/instances/old")).unwrap();
        std::fs::create_dir_all(data.join("asteria/instances/new")).unwrap();
        assert_eq!(run_in(&data, &config, || false).unwrap(), Outcome::Nothing);
        assert!(data.join("omacraft/instances/old").is_dir());
        assert!(data.join("asteria/instances/new").is_dir());
    }

    #[test]
    fn a_fresh_install_has_nothing_to_move() {
        let (data, config) = scratch("fresh");
        assert_eq!(run_in(&data, &config, || true).unwrap(), Outcome::Nothing);
    }
}
