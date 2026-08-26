use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("data/key_mapping_table.json");
    println!("cargo:rerun-if-changed={}", path.display());

    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let raw: HashMap<String, Vec<String>> = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()));

    let mut seen: HashMap<String, u32> = HashMap::new();
    for (key, value) in raw {
        let raw_val: u32 = match key.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let type_byte = (raw_val >> 24) as u8;
        if matches!(type_byte, 0 | 3 | 4) {
            continue; // KeyBoard/Macro/Custom — excluded from the label reverse index
        }
        let Some(label) = value.into_iter().next() else {
            continue;
        };
        if let Some(&existing) = seen.get(&label) {
            panic!(
                "key_mapping_table.json: label \"{label}\" is ambiguous (raw {existing} and {raw_val}) \
                 — KeyMappingCodec's label->raw index requires every non-KeyBoard/Macro/Custom label \
                 to be unique. Rename one of the labels or exclude its type."
            );
        }
        seen.insert(label, raw_val);
    }
}
