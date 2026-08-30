//! Host docker fake for `factory rig` tests.
#![allow(
    clippy::disallowed_types,
    reason = "test support: tokio mutexes over plain state"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::sync::Mutex;

/// Host docker fake: records compose calls, answers `ps` from a map, tracks archives.
#[derive(Debug, Default)]
pub struct FakeHostDocker {
    pub calls: Mutex<Vec<String>>,
    pub volumes: Mutex<std::collections::BTreeSet<String>>,
    pub networks: Mutex<std::collections::BTreeSet<String>>,
    pub ps: Mutex<BTreeMap<String, String>>,
    pub archived: Mutex<Vec<(String, PathBuf)>>,
    pub restored: Mutex<Vec<(String, PathBuf)>>,
    pub fail: bool,
}

impl FakeHostDocker {
    fn check(&self, what: &str) -> Result<(), crate::HostError> {
        if self.fail {
            return Err(crate::HostError::Command {
                what: what.to_owned(),
                detail: "fake".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl crate::HostDocker for FakeHostDocker {
    async fn compose(
        &self,
        project: &str,
        env_file: &Path,
        compose_file: &Path,
        args: &[&str],
    ) -> Result<String, crate::HostError> {
        self.check("compose")?;
        self.calls.lock().await.push(format!(
            "{project} {} {} {}",
            env_file.display(),
            compose_file.display(),
            args.join(" ")
        ));
        Ok(self
            .ps
            .lock()
            .await
            .get(project)
            .cloned()
            .unwrap_or_default())
    }
    async fn volume_exists(&self, name: &str) -> Result<bool, crate::HostError> {
        self.check("volume")?;
        Ok(self.volumes.lock().await.contains(name))
    }
    async fn network_exists(&self, name: &str) -> Result<bool, crate::HostError> {
        self.check("network")?;
        Ok(self.networks.lock().await.contains(name))
    }
    async fn archive_volume(&self, volume: &str, dest: &Path) -> Result<(), crate::HostError> {
        self.check("archive")?;
        self.archived
            .lock()
            .await
            .push((volume.to_owned(), dest.to_path_buf()));
        Ok(())
    }
    async fn restore_volume(&self, volume: &str, src: &Path) -> Result<(), crate::HostError> {
        self.check("restore")?;
        self.restored
            .lock()
            .await
            .push((volume.to_owned(), src.to_path_buf()));
        Ok(())
    }
}
