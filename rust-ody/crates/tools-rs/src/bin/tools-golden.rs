fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: tools-golden <fixture.json>")?;
    let results = tools_rs::golden::run_fixture_file(&path);
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
