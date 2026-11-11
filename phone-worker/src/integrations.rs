use crate::providers::{
    checked, encode, endpoint, http_client, read_json, required, ProviderError, Result,
};
use serde_json::{json, Value};

/// Explicit rejections did not accept the write; transport/5xx may have committed it.
pub fn delivery_write_error(error: ProviderError) -> ProviderError {
    if matches!(
        error.code.as_str(),
        "provider_rate_limited"
            | "provider_permission_denied"
            | "provider_reauthorization_required"
            | "provider_resource_missing"
            | "provider_request_rejected"
            | "provider_redirect_blocked"
    ) {
        error
    } else {
        ProviderError::permanent("delivery_uncertain")
    }
}
pub fn delivery_marker(org: uuid::Uuid, call: uuid::Uuid, analysis: Option<uuid::Uuid>) -> String {
    format!(
        "phone-platform:{org}:{call}:{}",
        analysis
            .map(|v| v.to_string())
            .unwrap_or_else(|| "call".into())
    )
}
fn account(config: &Value) -> Result<url::Url> {
    endpoint(
        &format!("https://{}", required(config, "account_domain")?),
        "amocrm",
    )
}

pub async fn deliver_amocrm(
    config: &Value,
    deal_id: &str,
    marker: &str,
    text: &str,
    uncertain: bool,
) -> Result<Value> {
    if deal_id.parse::<u64>().ok().filter(|v| *v > 0).is_none() {
        return Err(ProviderError::permanent("invalid_deal_id"));
    }
    let client = http_client()?;
    let base = account(config)?;
    let token = required(config, "access_token")?;
    let mut ended = false;
    for page in 1..=20 {
        let response = checked(
            client
                .get(
                    base.join(&format!("/api/v4/leads/{deal_id}/notes"))
                        .unwrap(),
                )
                .bearer_auth(token)
                .query(&[("limit", 250), ("page", page)])
                .send()
                .await,
        )
        .await?;
        if response.status().as_u16() == 204 {
            ended = true;
            break;
        }
        let data = read_json(response).await?;
        let notes = data["_embedded"]["notes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(note) = notes.iter().find(|n| {
            n["params"]["text"]
                .as_str()
                .is_some_and(|s| s.contains(marker))
        }) {
            return Ok(
                json!({"state":"delivered","external_id":note["id"].to_string(),"reconciled":true}),
            );
        }
        if notes.len() < 250 {
            ended = true;
            break;
        }
    }
    if !ended {
        return Err(ProviderError::retry("crm_reconciliation_incomplete", 60));
    }
    if uncertain {
        return Err(ProviderError::permanent("delivery_uncertain"));
    }
    let payload = json!([{"note_type":"common","params":{"text":format!("{marker}\n{text}")}}]);
    // Once the write starts, any ambiguous outcome must never cause an automatic second POST.
    let response = client
        .post(
            base.join(&format!("/api/v4/leads/{deal_id}/notes"))
                .unwrap(),
        )
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await;
    let response = checked(response).await.map_err(delivery_write_error)?;
    let data = read_json(response)
        .await
        .map_err(|_| ProviderError::permanent("delivery_uncertain"))?;
    let id = data["_embedded"]["notes"][0]["id"]
        .as_u64()
        .ok_or_else(|| ProviderError::permanent("delivery_uncertain"))?;
    Ok(json!({"state":"delivered","external_id":id.to_string(),"reconciled":false}))
}
/// Mapping is field -> spreadsheet column. Column A is always the tenant-scoped stable call key.
pub fn sheets_columns(
    config: &Value,
    call: &Value,
    analysis: Option<&Value>,
    public_url: &str,
    marker: &str,
) -> Result<Vec<Value>> {
    let defaults = json!({"call_id":"A","date":"B","phone":"C","contact_name":"D","employee":"E","direction":"F","result":"G","score":"H","summary":"I","call_link":"J","delivery_marker":"K"});
    let mapping = config
        .get("column_mapping")
        .filter(|v| !v.is_null())
        .unwrap_or(&defaults)
        .as_object()
        .ok_or_else(|| ProviderError::permanent("invalid_column_mapping"))?;
    if mapping.get("call_id").and_then(Value::as_str) != Some("A") || mapping.len() > 52 {
        return Err(ProviderError::permanent("invalid_column_mapping"));
    }
    let mut indexed = std::collections::BTreeMap::new();
    for (field, column) in mapping {
        let label = column
            .as_str()
            .ok_or_else(|| ProviderError::permanent("invalid_column_mapping"))?;
        if label.is_empty() || label.len() > 2 || !label.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(ProviderError::permanent("invalid_column_mapping"));
        }
        let index = label
            .bytes()
            .fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize)
            - 1;
        if index > 51 || (field != "call_id" && index == 0) || indexed.contains_key(&index) {
            return Err(ProviderError::permanent("invalid_column_mapping"));
        }
        let value = match field.as_str() {
            "call_id" => Value::Null,
            "date" | "started_at" => call["started_at"].clone(),
            "result" | "provider_result" => call["provider_result"].clone(),
            "summary" => analysis
                .map(|a| a["summary"].clone())
                .unwrap_or(Value::Null),
            "call_link" => json!(format!(
                "{}/calls/{}",
                public_url.trim_end_matches('/'),
                call["id"].as_str().unwrap_or("")
            )),
            "delivery_marker" => json!(marker),
            "phone" | "contact_name" | "employee" | "direction" | "duration" | "score"
            | "status_id" | "tag_ids" | "contact_kind" => call[field].clone(),
            _ => return Err(ProviderError::permanent("invalid_column_mapping")),
        };
        let value = if value.is_array() || value.is_object() {
            json!(value.to_string())
        } else if value.is_null() {
            json!("")
        } else {
            value
        };
        indexed.insert(index, value);
    }
    let last = *indexed.keys().max().unwrap();
    Ok((1..=last)
        .map(|i| indexed.remove(&i).unwrap_or(json!("")))
        .collect())
}
fn sheet_label(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 100 || value.chars().any(char::is_control) {
        return Err(ProviderError::permanent("invalid_sheet_name"));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}
pub async fn deliver_sheets(
    config: &Value,
    key: &str,
    columns: Vec<Value>,
    uncertain: bool,
) -> Result<Value> {
    let client = http_client()?;
    let token = required(config, "access_token")?;
    let id = encode(required(config, "spreadsheet_id")?);
    let sheet = sheet_label(config["sheet_name"].as_str().unwrap_or("Calls"))?;
    let range = format!("{sheet}!A:AZ");
    let read = endpoint(
        &format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{id}/values/{}",
            encode(&range)
        ),
        "google",
    )?;
    let data = read_json(checked(client.get(read).bearer_auth(token).send().await).await?).await?;
    let rows = data["values"].as_array().cloned().unwrap_or_default();
    if rows.len() > 100_000 {
        return Err(ProviderError::permanent("sheet_row_limit"));
    }
    let matching: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            if r[0].as_str() == Some(key) {
                Some(i + 1)
            } else {
                None
            }
        })
        .collect();
    if matching.len() > 1 {
        return Err(ProviderError::permanent("sheet_duplicate_call_key"));
    }
    let mut values = vec![json!(key)];
    values.extend(columns);
    let payload = json!({"majorDimension":"ROWS","values":[values]});
    if let Some(row) = matching.first() {
        let range = format!("{sheet}!A{row}");
        let url = endpoint(
            &format!(
                "https://sheets.googleapis.com/v4/spreadsheets/{id}/values/{}?valueInputOption=RAW",
                encode(&range)
            ),
            "google",
        )?;
        let response = checked(
            client
                .put(url)
                .bearer_auth(token)
                .json(&payload)
                .send()
                .await,
        )
        .await
        .map_err(delivery_write_error)?;
        let result = read_json(response)
            .await
            .map_err(|_| ProviderError::permanent("delivery_uncertain"))?;
        return Ok(
            json!({"state":"delivered","external_id":result["updatedRange"],"reconciled":uncertain}),
        );
    }
    if uncertain {
        return Err(ProviderError::permanent("delivery_uncertain"));
    }
    let range = format!("{sheet}!A1");
    let url=endpoint(&format!("https://sheets.googleapis.com/v4/spreadsheets/{id}/values/{}:append?valueInputOption=RAW&insertDataOption=INSERT_ROWS",encode(&range)),"google")?;
    let response = checked(
        client
            .post(url)
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await,
    )
    .await
    .map_err(delivery_write_error)?;
    let result = read_json(response)
        .await
        .map_err(|_| ProviderError::permanent("delivery_uncertain"))?;
    Ok(
        json!({"state":"delivered","external_id":result["updates"]["updatedRange"],"reconciled":false}),
    )
}
