pub struct InfluxConfig {
    pub url: String,
    pub database: String,
}

impl InfluxConfig {
    pub fn new(url: &str, database: &str) -> Self {
        Self {
            url: url.to_string(),
            database: database.to_string(),
        }
    }
}
