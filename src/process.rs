use anyhow::Context;
use async_trait::async_trait;
use bytes::{Buf};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::codec::Decoder;
use std::env::current_exe;
use std::fmt::Debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use ya_agreement_utils::OfferTemplate;
use crate::process::dummy::Config;

pub mod dummy;

#[allow(unused)]
#[derive(Default, Clone)]
pub struct Usage {
    pub cnt: u64,
}

#[async_trait]
pub(crate) trait Runtime: Sized {
    fn parse_config(config: &Option<Value>) -> anyhow::Result<Config> {
        match config {
            None => Ok(Config::default()),
            Some(config) => Ok(serde_json::from_value(config.clone())?),
        }
    }

    async fn start(mode: Option<PathBuf>, config: Config) -> anyhow::Result<Self>;

    async fn stop(&mut self) -> anyhow::Result<()>;

    async fn wait(&mut self) -> std::io::Result<ExitStatus>;

    fn test(_config: &Config) -> anyhow::Result<()>;

    fn offer_template(_config: &Config) -> anyhow::Result<OfferTemplate>;
}

pub(crate) trait RuntimeConfig: DeserializeOwned + Default + Debug + Clone {
    fn gpu_uuid(&self) -> Option<String>;
}

pub fn find_file(file_name: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let exe = current_exe()?;
    let parent_dir = exe
        .parent()
        .context("Unable to get parent dir of {exe:?}")?;
    let file = parent_dir.join(&file_name);
    if file.exists() {
        return Ok(file);
    }
    anyhow::bail!("Unable to get dummy runtime base dir");
}
