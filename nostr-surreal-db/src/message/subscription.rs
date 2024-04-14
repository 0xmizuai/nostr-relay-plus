use anyhow::{anyhow, Result};
use serde::Deserialize;

use super::{events::Event, filter::Filter, wire::FilterOnWire};

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: String,
    pub filters: Vec<FilterOnWire>,
}

impl Subscription {

    pub fn parse_filters(&self) -> Result<Vec<Filter>> {
        let filters = self.filters.iter()
                .map(|e| e.clone().try_into())
                .collect::<Vec<_>>();
        
        let mut result = Vec::new();
        for f in filters {
            result.push(f?);
        }
        Ok(result)
    }

    pub fn is_interested(&self, e: &Event) -> Result<()> {
        let filters = self.parse_filters()?;
        if e.match_filters(&filters) {
            Ok(())
        } else {
            Err(anyhow!("filter dose not match"))
        }
    }
}