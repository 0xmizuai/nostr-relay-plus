use anyhow::Result;

use crate::message::events::Event; 
use crate::message::filter::Filter;
use crate::sql_builder::SqlBuilder;
use crate::DB;

impl DB {
    pub async fn write_event(&self, event: &Event) -> Result<()> {
        let sql = SqlBuilder::new()
            .write_event(&event)
            .build();

        println!("Write Query: {sql}");
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

    pub async fn query_by_filters(&self, filters: &[Filter]) -> Result<Vec<Event>> {
        let sql = SqlBuilder::new()
            .get_events_by_filters(filters)
            .build();

        let events = self.db
            .query(sql).await?
            .take::<Vec<Event>>(0)?
            .iter()
            .filter(|event| event.match_many_filters(filters))
            .map(|e| e.clone())
            .collect::<Vec<_>>();

        Ok(events)
    }
}