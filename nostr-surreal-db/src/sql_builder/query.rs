use serde_json::json;

use crate::message::events::Event;
use crate::message::filter::Filter;
use crate::types::{Bytes32, NOSTR_EVENTS_TABLE};

use super::SqlBuilder;

impl SqlBuilder {
    pub fn write_event(&mut self, event: &Event) -> &mut Self {

        let sql = format!("
            CREATE {NOSTR_EVENTS_TABLE} SET
                id = '{}',
                sender = '{}',
                sig = '{}',
                
                create_at = {},
                kind = {},
                tags = '{}',

                expieration = {},
                content = '{}',
                raw_event = '{}';
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
            event.raw_event,
        );

        self.push_sql(sql);
        self
    }

    pub fn update_event(&mut self, event: &Event) -> &mut Self {
        let sql = format!("
            UPDATE {NOSTR_EVENTS_TABLE} SET
                id = '{}',
                sender = '{}',
                sig = '{}',

                create_at = {},
                kind = {},
                tags = '{}',

                expieration = {},
                content = '{}',
                raw_event = '{}';
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
            event.raw_event,
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

    pub fn get_events_by_filters(&mut self, filters: &[Filter]) -> &mut Self {
        // TODO: we are skipping tags for now ... as it is very complicated
        // Instead, we check on the results from the other queries

        println!("FILTERS {:?}", filters);
        assert!(filters.len() > 0);

        let mut sql = format!("
            SELECT * FROM {NOSTR_EVENTS_TABLE} WHERE
        ");

        let mut filter_queries = Vec::new();
        for filter in filters {
            let all_filter_query = {
                let mut filter_query = "( true ".to_string();
                let mut has_any_filter = false;
    
                let mut id_filter = Vec::new();
                for id in &filter.ids {
                    id_filter.push(format!("{},", hex::encode(id)));
                }
                if id_filter.len() > 0 {
                    let id_filter_query = id_filter.join("");
                    filter_query.push_str(&format!("id in [{}]", id_filter_query));
                    has_any_filter = true;
                }
    
                let mut sender_filter = Vec::new();
                for sender in &filter.authors {
                    sender_filter.push(format!("{},", hex::encode(sender.to_bytes())));
                }
                if sender_filter.len() > 0 {
                    let sender_filter_query = sender_filter.join("");
                    filter_query.push_str(&format!("AND sender in [{}]", sender_filter_query));
                    has_any_filter = true;
                }
    
                let mut kind_filter = Vec::new();
                for kind in &filter.kinds {
                    kind_filter.push(format!("{},", kind));
                }
                if kind_filter.len() > 0 {
                    let kind_filter_query = kind_filter.join("");
                    filter_query.push_str(&format!("AND kind in [{}]", kind_filter_query));
                    has_any_filter = true;
                }
                filter_query.push_str(")");
    
                if has_any_filter {
                    filter_query
                } else {
                    "".to_string()
                }
            };
    
            // Time
            let the_time_query = {
                let mut time_query = Vec::new();
    
                if filter.since.is_some() {
                    time_query.push(format!("create_at > {}", filter.since.unwrap()));
                }
        
                if filter.until.is_some() {
                    time_query.push(format!("create_at < {}", filter.until.unwrap()));
                }
    
                match time_query.len() {
                    0 => "created_at > 0".to_string(),
                    1 => time_query[0].to_string(),
                    2 => format!("({} AND {})", time_query[0], time_query[1]),
                    _ => panic!("unrechable")
                }
            };
            
            if all_filter_query.len() > 0 {
                filter_queries.push(format!("({} AND {})", all_filter_query, the_time_query));
            } else {
                filter_queries.push(format!("({})", the_time_query));
            }
        }
        
        sql.push_str(&filter_queries.join(" OR "));
        sql.push_str(";");

        println!("SQL {:?}", sql);
        self.push_sql(sql);
        self
    }
}