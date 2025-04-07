use ya_agreement_utils::OfferTemplate;

pub(crate) fn template() -> anyhow::Result<OfferTemplate> {
    let offer_template = include_bytes!("offer-template.json");
    let template: OfferTemplate = serde_json::from_slice(offer_template.as_ref())?;
    Ok(template)
}
