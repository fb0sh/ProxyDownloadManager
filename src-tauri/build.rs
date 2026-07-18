use std::fs;
use std::path::Path;

fn main() {
    tauri_build::build();

    // Generate event name constants from events.json
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let events_path = Path::new(&manifest_dir).join("events.json");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("generated_events.rs");

    eprintln!("[build.rs] Reading events from: {:?}", events_path);
    eprintln!("[build.rs] Writing to: {:?}", out_path);

    let content = match fs::read_to_string(&events_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[build.rs] Failed to read events.json: {}", e);
            return;
        }
    };

    let events: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[build.rs] Failed to parse events.json: {}", e);
            return;
        }
    };

    let mut code = String::from("// Auto-generated from events.json — do not edit manually\n\n");
    if let Some(obj) = events.as_object() {
        for (key, value) in obj {
            let const_name = key.to_uppercase();
            let event_name = value.as_str().unwrap_or("");
            code.push_str(&format!("pub const {}: &str = \"{}\";\n", const_name, event_name));
        }
    }

    if let Err(e) = fs::write(&out_path, code) {
        eprintln!("[build.rs] Failed to write generated_events.rs: {}", e);
    } else {
        eprintln!("[build.rs] Successfully generated events");
    }

    // Re-run if events.json changes
    println!("cargo:rerun-if-changed=events.json");

    // Validate frontend copy matches source
    let frontend_events = Path::new(&manifest_dir).parent().unwrap().join("src/constants/events.json");
    if let Ok(frontend_content) = fs::read_to_string(&frontend_events) {
        if frontend_content.trim() != content.trim() {
            panic!(
                "events.json mismatch: src-tauri/events.json and src/constants/events.json differ. \
                 Edit src-tauri/events.json (source of truth) and copy to src/constants/events.json."
            );
        }
    }
}
