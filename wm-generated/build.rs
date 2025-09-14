use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::{env, fs};

use fancy_regex::Regex;

struct VecPushGuard<'a, T> {
    _collection: &'a mut Vec<T>,
}

impl<'a, T> VecPushGuard<'a, T> {
    fn new(collection: &'a mut Vec<T>, item: T) -> Self {
        collection.push(item);
        Self {
            _collection: collection,
        }
    }

    fn collection(&self) -> &[T] {
        self._collection
    }

    fn collection_mut(&mut self) -> &mut Vec<T> {
        self._collection
    }
}

impl<T> Drop for VecPushGuard<'_, T> {
    fn drop(&mut self) {
        self._collection.pop();
    }
}

fn key_to_qualifier(name: &str) -> String {
    let parts = name.split("_");
    parts
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join("")
}

fn process_object(
    key: String,
    data: &serde_json::Value,
    rust_identifier: &Regex,
    qualified_path: &mut Vec<String>,
) -> (String, String) {
    let mut append = VecPushGuard::new(qualified_path, key);
    let struct_name = append
        .collection()
        .iter()
        .map(|part| key_to_qualifier(part))
        .collect::<Vec<String>>()
        .join("_");
    // assert!(rust_identifier.is_match(&struct_name).unwrap());

    let properties = match data.get("properties") {
        Some(props) => props.as_object().unwrap(),
        None => return ("serde_json::Value".to_string(), String::new()),
    };

    let mut code = String::new();
    code.push_str("#[allow(non_camel_case_types)]\n");
    code.push_str("#[derive(Debug, Deserialize, Serialize)]\n");
    code.push_str(&format!("pub struct {struct_name} {{\n"));

    let mut other_defines = vec![];
    let mut field_names_to_structs = HashMap::new();

    let mut has_timestamp = false;
    for (attribute, props) in properties {
        let mut serde_macro = vec![];
        let mut field_name = attribute.clone();
        if !rust_identifier.is_match(attribute).unwrap() {
            serde_macro.push(format!("rename = \"{attribute}\""));
            field_name = attribute
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();

            if !rust_identifier.is_match(&field_name).unwrap() {
                field_name.push('_');
            }
        }

        let elastic_type = props
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("object");

        let mut rust_type = match elastic_type {
            "boolean" => "bool".to_string(),
            "byte" => "i8".to_string(),
            "date" => "DateTime<Utc>".to_string(),
            "double" | "scaled_float" => "f64".to_string(),
            "float" => "f32".to_string(),
            "geo_point" => "(f64, f64)".to_string(),
            "half_float" => "f16".to_string(),
            "integer" => "i32".to_string(),
            "ip" => "IpAddr".to_string(),
            "keyword" | "text" | "wildcard" => "Vec<String>".to_string(),
            "long" => "i64".to_string(),
            "short" => "i16".to_string(),
            "unsigned_long" => "u64".to_string(),
            "object" => {
                let (nested_type, nested_code) = process_object(
                    field_name.clone(),
                    props,
                    rust_identifier,
                    append.collection_mut(),
                );

                if !nested_code.is_empty() {
                    other_defines.push(nested_code);
                }
                nested_type
            }
            _ => "serde_json::Value".to_string(),
        };

        if attribute == "@timestamp" {
            has_timestamp = true;
        } else {
            serde_macro.push("skip_serializing_if = \"Option::is_none\"".to_string());
            rust_type = format!("Option<{rust_type}>");
            field_names_to_structs.insert(field_name.clone(), rust_type.clone());
        }

        code.push_str(&format!("    #[serde({})]\n", serde_macro.join(", ")));
        code.push_str(&format!("    pub {field_name}: "));
        code.push_str(&format!("{rust_type},\n"));
    }

    code.push_str("}\n\n");
    code.push_str(&format!("impl {struct_name} {{\n"));

    if has_timestamp {
        code.push_str("    pub fn new(timestamp: DateTime<Utc>) -> Self {\n");
    } else {
        code.push_str("    pub fn new() -> Self {\n");
    }

    code.push_str("        Self {\n");

    if has_timestamp {
        code.push_str("            timestamp,\n");
    }

    for (field_name, _) in field_names_to_structs {
        code.push_str(&format!("            {field_name}: None,\n"));
    }

    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    if !has_timestamp {
        code.push_str(&format!("\nimpl Default for {struct_name} {{\n"));
        code.push_str("    fn default() -> Self {\n");
        code.push_str("        Self::new()\n");
        code.push_str("    }\n");
        code.push_str("}\n");
    }

    if !other_defines.is_empty() {
        code.push('\n');
        code.push_str(&other_defines.join("\n"));
    }

    (struct_name, code)
}

fn main() {
    let env_cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let env_out_dir = env::var("OUT_DIR").unwrap();

    let wm_generated_dir = Path::new(&env_cargo_manifest_dir);
    let workspace_dir = wm_generated_dir.parent().unwrap();
    let out_dir = Path::new(&env_out_dir);

    let source = workspace_dir.join("config").join("ecs-template.json");
    println!("cargo:rerun-if-changed={}", source.display());

    let input_file = fs::File::open(source).unwrap();
    let mut output_file = fs::File::create(out_dir.join("ecs.rs")).unwrap();

    output_file.write_all(b"use std::net::IpAddr;\n\n").unwrap();
    output_file
        .write_all(b"use chrono::{DateTime, Utc};\n")
        .unwrap();
    output_file
        .write_all(b"use serde::{Deserialize, Serialize};\n\n")
        .unwrap();

    let rust_identifier =
        Regex::new(r"^(?!(?:as|async|await|break|const|continue|crate|dyn|else|enum|extern|false|fn|for|if|impl|in|let|loop|match|mod|move|mut|pub|ref|return|self|Self|static|struct|super|trait|true|type|unsafe|use|where|while)$)[a-zA-Z_][a-zA-Z0-9_]*$")
            .unwrap();
    let data = serde_json::from_reader::<_, serde_json::Value>(input_file).unwrap();

    let mut qualified_path = vec![];
    output_file
        .write_all(
            process_object(
                "ECS".into(),
                &data["template"]["mappings"],
                &rust_identifier,
                &mut qualified_path,
            )
            .1
            .as_bytes(),
        )
        .unwrap();
}
