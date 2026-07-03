use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: kaos-golden <fixture.json>")?;
    let results = kaos_rs::golden::run_fixture_file_async(&path).await?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
