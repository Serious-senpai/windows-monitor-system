use std::error::Error;

use log::debug;
use reqwest::header::USER_AGENT;
use serde_json::Value;
use wm_common::schema::github::GitHubDirectoryEntry;

fn _extract_key(value: &mut Value, key: &str) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .remove(key)
        .unwrap_or_else(|| panic!("Cannot find key \"{key}\""))
}

async fn _query_rule_toml(
    client: reqwest::Client,
    entry: GitHubDirectoryEntry,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let response = client.get(&entry.download_url).send().await?;
    let data = response.bytes().await?;
    let mut toml = toml::from_slice::<Value>(&data)?;

    let mut rule = _extract_key(&mut toml, "rule");
    let old_rule_id = rule["rule_id"]
        .as_str()
        .expect("Original rule_id is not a String")
        .to_string();

    let mut references = rule["references"]
        .as_array()
        .map(|v| v.clone())
        .unwrap_or_default();
    references.push(entry.html_url.into());

    rule["rule_id"] = format!("custom-{old_rule_id}").into(); // Trick Kibana into thinking that this is not a prebuilt rule
    rule["references"] = references.into();
    rule["enabled"] = true.into();
    rule["index"] = vec![".ds-events.windows-monitor-ecs-*"].into();

    // Field transform (possible bug in elastic/detection-rules?)
    if let Some(mut new_terms) = rule["new_terms"].as_object_mut().cloned() {
        let field = new_terms["field"]
            .as_str()
            .expect("Original new_terms.field is not a String")
            .to_string();
        rule[field] = new_terms.remove("value").unwrap_or_default();

        if let Some(mut history_window_start) = new_terms.remove("history_window_start") {
            if let Some(pairs) = history_window_start.as_array_mut() {
                for pair in pairs {
                    let field = pair["field"]
                        .as_str()
                        .expect(
                            "Original new_terms.history_window_start.<index>.field is not a String",
                        )
                        .to_string();

                    rule[field] = _extract_key(pair, "value");
                }
            }
        }
    }

    Ok(rule)
}

pub async fn fetch_remote_rules() -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/elastic/detection-rules/contents/rules/windows?ref=9.1")
        .header(USER_AGENT, "windows-monitor-system")
        .send()
        .await?;
    let json = response.json::<Vec<GitHubDirectoryEntry>>().await?;

    let mut tasks = vec![];
    for entry in json {
        tasks.push(tokio::spawn(_query_rule_toml(client.clone(), entry)));
    }

    let mut objects = vec![];
    for task in tasks {
        let rule = task.await??;
        debug!("Fetched rule {rule:?}");
        objects.push(rule);
    }

    Ok(objects)
}
