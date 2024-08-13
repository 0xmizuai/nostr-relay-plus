use redis_macros::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, FromRedisValue, ToRedisArgs, Clone)]
pub struct RedisEventOnWire {
    pub id: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    pub content: String,
}
