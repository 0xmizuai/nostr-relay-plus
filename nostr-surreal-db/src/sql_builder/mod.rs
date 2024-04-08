pub mod query;

#[derive(Debug, Clone)]
pub struct SqlBuilder {
    pub(crate) sql: Vec<String>,
}

impl SqlBuilder {

    pub fn new() -> Self {
        SqlBuilder { sql: Vec::new() }
    }

    pub fn push_sql(&mut self, sql: String) -> &mut Self {
        self.sql.push(sql);
        self
    }

    pub fn build(&self) -> String {
        self.sql.join("\n")
    }
}
