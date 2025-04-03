use std::fs;
use std::path::PathBuf;
use std::process::ExitStatus;

use async_trait::async_trait;
use serde::Deserialize;

use ya_agreement_utils::OfferTemplate;

use crate::offer_template;

use super::{Runtime, RuntimeConfig};

const OFFER_OVERRIDE_FILE_PATH_ENV: &str = "OFFER_OVERRIDE_FILE_PATH";

#[derive(Clone)]
pub struct Dummy {
}

#[derive(Deserialize, Clone, Debug, Default)]
pub(crate) struct Config {
    #[allow(dead_code)]
    pub dummy_arg: Option<String>,
}

impl RuntimeConfig for Config {
    fn gpu_uuid(&self) -> Option<String> {
        None
    }
}

#[async_trait]
impl Runtime for Dummy {
    type CONFIG = Config;

    async fn start(model: Option<PathBuf>, _config: Self::CONFIG) -> anyhow::Result<Dummy> {
        panic!("Dummy runtime is not implemented yet");
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        panic!("Dummy runtime is not implemented yet");
    }

    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        panic!("Dummy runtime is not implemented yet");
    }

    fn test(_config: &Self::CONFIG) -> anyhow::Result<()> {
        Ok(())
    }

    fn offer_template(config: &Self::CONFIG) -> anyhow::Result<OfferTemplate> {
        let template = offer_template::template(config)?;
        if let Ok(Some(overrides)) = Dummy::read_overrides() {
            Ok(template.patch(overrides))
        } else {
            Ok(template)
        }
    }
}

impl Dummy {
    fn read_overrides() -> anyhow::Result<Option<OfferTemplate>> {
        if let Ok(override_json_path) = std::env::var(OFFER_OVERRIDE_FILE_PATH_ENV) {
            let file = fs::File::open(override_json_path)?;
            let overrides: OfferTemplate = serde_json::from_reader(file)?;
            Ok(Some(overrides))
        } else {
            Ok(None)
        }
    }
}
