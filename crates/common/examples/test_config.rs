use pharmakon_common::Config;

fn main() {
    let config = Config::load().unwrap_or_default();
    println!("Loaded config: {:?}", config);

    let mut new_config = config.clone();
    new_config.default_agent.provider = "test-provider".to_string();
    new_config.save().unwrap();
    println!("Saved config to file.");

    let reloaded = Config::load().unwrap();
    println!("Reloaded config: {:?}", reloaded);
}
