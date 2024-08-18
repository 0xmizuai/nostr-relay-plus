use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    // Get tracing configuration from env var or use default
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "full".to_string());

    let subscriber = tracing_subscriber::fmt().with_env_filter(env_filter);

    match format.as_str() {
        "json" => subscriber.json().init(),
        "compact" => subscriber.compact().init(),
        "pretty" => subscriber.pretty().init(),
        _ => subscriber.init(),
    }
}
