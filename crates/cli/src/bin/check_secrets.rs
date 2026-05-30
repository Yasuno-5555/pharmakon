use pharmakon_common::SecretStore;

fn main() {
    let store = SecretStore::new();
    let providers = vec![
        "GEMINI",
        "OPENAI",
        "ANTHROPIC",
        "GROQ",
        "PERPLEXITY",
        "DEEPSEEK",
    ];

    println!("=== Pharmakon Secret Check ===");
    for p in providers {
        let name = format!("{}_API_KEY", p);
        match store.get_secret(&name) {
            Ok(key) => println!("{}: OK (length: {})", name, key.len()),
            Err(e) => println!("{}: MISSING ({})", name, e),
        }
    }
}
