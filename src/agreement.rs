use anyhow::anyhow;
use std::path::Path;

use ya_agreement_utils::AgreementView;

#[derive(Clone)]
pub struct AgreementDesc {
    pub counters: Vec<String>,
}

impl AgreementDesc {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let agreement = AgreementView::try_from(&path).map_err(|e| {
            anyhow!(
                "Failed to load Agreement from: {} Error: {e}",
                path.display()
            )
        })?;
        let counters: Vec<String> = agreement
            .pointer_typed("/offer/properties/golem/com/usage/vector")
            .map_err(|e| anyhow!("Invalid Agreement: Error loading usage counters: {e}"))?;
        //let model: String = agreement
        //    .pointer_typed("/demand/properties/golem/srv/comp/ai/model")
        //    .map_err(|e| anyhow!("Invalid Agreement: Failed to find ai model: {e}"))?;

        Ok(AgreementDesc { counters })
    }
}

#[cfg(test)]
mod tests {
    use crate::agreement::AgreementDesc;

    use std::path::PathBuf;

    fn test_agreement_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/resources/agreement.json")
    }
    #[test]
    fn test_loading_agreement() {
        let agreement_path = test_agreement_path();
        let desc = AgreementDesc::load(agreement_path).unwrap();
        let usage = ["golem.usage.tera-hash", "golem.usage.duration_sec"];

        assert_eq!(desc.counters[0], usage[0]);
        assert_eq!(desc.counters[1], usage[1]);
    }
}
