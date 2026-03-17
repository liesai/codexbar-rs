use anyhow::Result;
use std::collections::BTreeMap;

use super::{ProviderConfig, StatusRequest, UsageSnapshot, create_provider, provider_names};

pub async fn fetch_usage(request: StatusRequest) -> Result<BTreeMap<String, UsageSnapshot>> {
    let mut usage_map = BTreeMap::new();

    for &name in provider_names() {
        let provider = create_provider(name, ProviderConfig::default())?;
        let usage = provider.status(request).await?;
        usage_map.insert(provider.name().to_string(), usage);
    }

    Ok(usage_map)
}
