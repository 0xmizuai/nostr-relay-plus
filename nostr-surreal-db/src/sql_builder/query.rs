use serde_json::json;
use nostr_plus_common::types::Bytes32;

use crate::message::events::Event;
use crate::message::filter::Filter;
use crate::types::{Tags, NOSTR_EVENTS_TABLE, NOSTR_TAGS_TABLE};

use super::SqlBuilder;

impl SqlBuilder {
    pub fn write_event(&mut self, event: &Event) -> &mut Self {
        let sql = format!("
            CREATE {NOSTR_EVENTS_TABLE} SET
                id = '{}',
                sender = '{}',
                sig = '{}',
                
                created_at = {},
                kind = {},
                tags = {},

                expieration = {},
                content = '{}';
        ", 
            hex::encode(&event.id),
            hex::encode(&event.sender.to_bytes()),
            hex::encode(&event.sig),
            event.created_at,
            event.kind,
            json!(event.tags).to_string(),
            match event.expiration {
                Some(expiration) => expiration.to_string(),
                None => "NULL".to_string(),
            },
            event.content,
        );

        self.push_sql(sql);
        self
    }

    pub fn write_tags(&mut self, event_id: &Bytes32, tags: &Tags) -> &mut Self {
        
        for tag in tags {
            let sql = format!("
                CREATE {NOSTR_TAGS_TABLE} SET
                    event_id = '{NOSTR_EVENTS_TABLE}:{}',
                    tag_name = '{}',
                    tag_value = '{}';
            ",
                hex::encode(event_id),
                tag.0, tag.1,
            );

            self.push_sql(sql);
        }

        self
    }

    pub fn update_event(&mut self, event: &Event) -> &mut Self {
        let sql = format!("
            UPDATE {NOSTR_EVENTS_TABLE} SET
                id = '{}',
                sender = '{}',
                sig = '{}',

                created_at = {},
                kind = {},
                tags = '{}',

                expieration = {},
                content = '{}';
        ",
            hex::encode(&event.id),
            hex::encode(&event.sender.to_bytes()),
            hex::encode(&event.sig),
            event.created_at,
            event.kind,
            json!(event.tags).to_string(),
            match event.expiration {
                Some(expiration) => expiration.to_string(),
                None => "NULL".to_string(),
            },
            event.content,
        );

        self.push_sql(sql);
        self
    }

    pub fn delete_event(&mut self, event_id: &str) -> &mut Self {
        let sql = format!("
            DELETE FROM {NOSTR_EVENTS_TABLE} WHERE id = '{}';
        ", event_id);

        self.push_sql(sql);
        self
    }

    pub fn get_event_by_author_kinds(&mut self, author: &Bytes32, kind: u16) -> &mut Self {
        let sql = format!("
            SELECT * FROM {NOSTR_EVENTS_TABLE} WHERE
                sender = '{}' AND
                kind = {};
        ",
            hex::encode(&author),
            kind,
        );

        self.push_sql(sql);
        self
    }

    pub fn get_event_by_id(&mut self, event_id: &Bytes32) -> &mut Self {
        let sql = format!("
            SELECT * FROM {NOSTR_EVENTS_TABLE} WHERE id = '{}';
        ", hex::encode(event_id));

        self.push_sql(sql);
        self
    }

    fn build_one_filter(filter: Filter) -> Vec<String> {
        let mut filter_query = Vec::new();

        let mut id_filter = Vec::new();
        for id in &filter.ids {
            id_filter.push(format!("'{}',", hex::encode(id)));
        }
        if id_filter.len() > 0 {
            let id_filter_query = id_filter.join("");
            filter_query.push(format!("meta::id(id) in [{}]", id_filter_query));
        }

        let mut sender_filter = Vec::new();
        for sender in &filter.authors {
            sender_filter.push(format!("'{}',", hex::encode(sender.to_bytes())));
        }
        if sender_filter.len() > 0 {
            let sender_filter_query = sender_filter.join("");
            filter_query.push(format!("sender in [{}]", sender_filter_query));
        }

        let mut kind_filter = Vec::new();
        for kind in &filter.kinds {
            kind_filter.push(format!("{},", kind));
        }
        if kind_filter.len() > 0 {
            let kind_filter_query = kind_filter.join("");
            filter_query.push(format!("kind in [{}]", kind_filter_query));
        }

        // Time
        let the_time_query = {
            let mut time_query = Vec::new();

            if filter.since.is_some() {
                time_query.push(format!("created_at > {}", filter.since.unwrap()));
            }
    
            if filter.until.is_some() {
                time_query.push(format!("created_at < {}", filter.until.unwrap()));
            }

            match time_query.len() {
                0 => "created_at > 0".to_string(),
                1 => time_query[0].to_string(),
                2 => format!("({} AND {})", time_query[0], time_query[1]),
                _ => unreachable!()
            }
        };

        filter_query.push(the_time_query);   

        filter_query
    }

    pub fn get_events_by_filters(&mut self, filters: &[Filter]) -> &mut Self {
        // TODO: we are skipping tags for now ... as it is very complicated
        // Instead, we check on the results from the other queries
        assert!(filters.len() > 0);

        let mut sql = format!("SELECT *, meta::id(id) as id FROM {NOSTR_EVENTS_TABLE} WHERE ");
        let mut all_filter_queries = Vec::new();
        for filter in filters {
            let filter_query = Self::build_one_filter(filter.clone());
            all_filter_queries.push(filter_query.join(" AND "));
        }

        if all_filter_queries.len() > 0 {
            let all_filter_query = all_filter_queries.iter()
                .map(|q| format!("({q})"))
                .collect::<Vec<String>>()
                .join(" OR ");

            sql.push_str(&all_filter_query);
        }
        sql.push_str(";");

        self.push_sql(sql);
        self
    }
}


#[cfg(test)]
mod tests {
    use nostr_plus_common::wire::FilterOnWire;

    use super::*;
    #[test]
    fn test_query() {
        // "ids": ["abababababababababababababababababababababababababababababababab", "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd", "1212121212121212121212121212121212121212121212121212121212121212"],

        let note = r###"
        {
            "authors": ["010000000000000000000000000000000000000000000000", "02000000010000000000000000000000000000000000000000000000"],
            "kinds": [2, 1],
            "until": 5,
            "since": 3,
            "limit": 6,
            "#d": ["ab", "cd", "12"],
            "#f": ["ab", "cd", "12", "ab"],
            "#b": [],
            "search": "abc",
            "invalid": ["ab", "cd", "12"],
            "_invalid": 123
          }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        println!("{:?}", filter);


        let sql = SqlBuilder::new()
            .get_events_by_filters(&[filter])
            .build();

        println!("SQL {sql}");
    }

}