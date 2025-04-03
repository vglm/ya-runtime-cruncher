use std::fs;

use serde::Deserialize;
use serde_json::Value;
use ya_agreement_utils::OfferTemplate;

use crate::offer_template;

const OFFER_OVERRIDE_FILE_PATH_ENV: &str = "OFFER_OVERRIDE_FILE_PATH";

#[derive(Clone)]
pub struct Dummy {}

#[derive(Deserialize, Clone, Debug, Default)]
pub(crate) struct Config {
    #[allow(dead_code)]
    pub dummy_arg: Option<String>,
}

impl Config {
    pub(crate) fn gpu_uuid(&self) -> Option<String> {
        None
    }
}

impl Dummy {
    pub fn test(_config: &Config) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn offer_template(config: &Config) -> anyhow::Result<OfferTemplate> {
        let template = offer_template::template(config)?;
        if let Ok(Some(overrides)) = Dummy::read_overrides() {
            Ok(template.patch(overrides))
        } else {
            Ok(template)
        }
    }
    pub fn parse_config(config: &Option<Value>) -> anyhow::Result<Config> {
        match config {
            None => Ok(Config::default()),
            Some(config) => Ok(serde_json::from_value(config.clone())?),
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
