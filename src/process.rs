use anyhow::Context;
use async_trait::async_trait;
use bytes::{Buf, BytesMut};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::codec::Decoder;

use std::cell::RefCell;
use std::env::current_exe;
use std::fmt::Debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::pin::Pin;
use std::process::ExitStatus;
use std::rc::Rc;
use std::task::Poll;

use ya_agreement_utils::OfferTemplate;


pub mod dummy;

#[allow(unused)]
#[derive(Default, Clone)]
pub struct Usage {
    pub cnt: u64,
}

#[async_trait]
pub(crate) trait Runtime: Sized {
    type CONFIG: RuntimeConfig;

    fn parse_config(config: &Option<Value>) -> anyhow::Result<Self::CONFIG> {
        match config {
            None => Ok(Self::CONFIG::default()),
            Some(config) => Ok(serde_json::from_value(config.clone())?),
        }
    }

    async fn start(mode: Option<PathBuf>, config: Self::CONFIG) -> anyhow::Result<Self>;

    async fn stop(&mut self) -> anyhow::Result<()>;

    async fn wait(&mut self) -> std::io::Result<ExitStatus>;

    fn test(_config: &Self::CONFIG) -> anyhow::Result<()> {
        panic!("unimplemented test");
    }

    fn offer_template(_config: &Self::CONFIG) -> anyhow::Result<OfferTemplate> {
        panic!("unimplemented test");
    }
}

pub(crate) trait RuntimeConfig: DeserializeOwned + Default + Debug + Clone {
    fn gpu_uuid(&self) -> Option<String>;
}

#[derive(Clone)]
pub(crate) struct ProcessController<T: Runtime + 'static> {
    inner: Rc<RefCell<ProcessControllerInner<T>>>,
}

#[allow(clippy::large_enum_variant)]
enum ProcessControllerInner<T: Runtime + 'static> {
    Deployed,
    Working { child: T },
    Stopped,
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

impl<RUNTIME: Runtime + Clone + 'static> ProcessController<RUNTIME> {
    pub fn new() -> Self {
        ProcessController {
            inner: Rc::new(RefCell::new(ProcessControllerInner::Deployed {})),
        }
    }

    pub fn report(&self) -> Option<()> {
        match *self.inner.borrow_mut() {
            ProcessControllerInner::Deployed { .. } => Some(()),
            ProcessControllerInner::Working { .. } => Some(()),
            _ => None,
        }
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let () = self.report().unwrap_or_default();
        let old = self.inner.replace(ProcessControllerInner::Stopped {});
        if let ProcessControllerInner::Working { mut child, .. } = old {
            return child.stop().await;
        }
        Ok(())
    }

}

