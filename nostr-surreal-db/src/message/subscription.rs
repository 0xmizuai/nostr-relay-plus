use serde::Deserialize;

use super::wire::FilterOnWire;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: String,
    pub filters: Vec<FilterOnWire>,
}
