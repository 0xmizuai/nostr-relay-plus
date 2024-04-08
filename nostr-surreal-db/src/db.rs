use anyhow::Result;

use crate::message::events::Event; 
use crate::message::filter::Filter;
use crate::sql_builder::SqlBuilder;
use crate::DB;

impl DB {
    pub async fn write_event(&self, event: &Event) -> Result<()> {
        let sql = SqlBuilder::new()
            .write_event(&event)
            .write_tags(&event.id, &event.tags)
            .build();

        self.db.query(sql).await?;
        Ok(())
    }

    pub async fn update_event(&self, event: &Event) -> Result<()> {
        let sql = SqlBuilder::new()
            .update_event(&event)
            .build();

        self.db.query(sql).await?;
        Ok(())
    }

    pub async fn query_by_filter(&self, filter: &Filter) -> Result<Vec<Event>> {
        let sql = SqlBuilder::new()
            .get_events_by_filter(filter)
            .build();

        let events = self.db
            .query(sql).await?
            .take::<Vec<Event>>(0)?
            .iter()
            .filter(|event| event.match_filter(filter))
            .map(|e| e.clone())
            .collect::<Vec<_>>();

        Ok(events)
    }
}


#[cfg(test)]
mod tests {
    use crate::message::wire::{EventOnWire, FilterOnWire};

    use super::*;

    // #[tokio::test]
    // async fn clear_db() {
    //     let db = DB::local_connect().await.unwrap();
    //     db.clear_db().await.unwrap();
    // }

    // #[tokio::test]
    // async fn test_write_event() {
    //     let note = r#"
    //     {
    //         "content": "Good morning everyone 😃",
    //         "created_at": 1680690006,
    //         "id": "eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d",
    //         "kind": 1,
    //         "sender": "010000000000000000000000000000000000000000000000",
    //         "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
    //         "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
    //       }
    //     "#;
    //     let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
    //     let event: Event = raw_event.try_into().unwrap();

    //     let db = DB::local_connect().await.unwrap();
    //     db.write_event(&event).await.unwrap();
    // }

    #[tokio::test]
    async fn test_query_filter_1() {
        let note = r###"
        {
            "ids": ["eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d", "0000000000000000000000000000000000000000000000000000000000000000"],
            "authors": ["010000000000000000000000000000000000000000000000", "000000000000000000000000000000000000000000000000000000000000000000000000"],
            "kind": [1, 2],
            "#t": ["nostr", "other"],
            "#subject": ["db", "other"],
            "since": 1680690000,
            "util": 2680690000
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 1);
    }

    #[tokio::test]
    async fn test_query_filter_2() {
        let note = r###"
        {
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 1);
    }

    #[tokio::test]
    async fn test_query_filter_3() {
        let note = r###"
        {
            "ids": ["eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d"]
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 1);
    }

    #[tokio::test]
    async fn test_query_filter_4() {
        let note = r###"
        {
            "ids": ["abababababababababababababababababababababababababababababababab"]
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn test_query_filter_5() {
        let note = r###"
        {
            "#t": ["nostr"],
            "#r": ["nostr"]
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn test_query_filter_6() {
        let note = r###"
        {
            "#t": ["other"]
        }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note).unwrap();
        let filter: Filter = filter.try_into().unwrap();

        let db = DB::local_connect().await.unwrap();
        let res = db.query_by_filter(&filter).await.unwrap();
        assert_eq!(res.len(), 0);
    }
}