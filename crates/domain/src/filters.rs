use chrono::DateTime;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Number,
    Date,
    Boolean,
    StringArray,
}
fn field_kind(field: &str) -> Option<Kind> {
    Some(match field {
        "direction" | "provider_result" | "employee" | "status_id" | "contact_kind" | "stage"
        | "callback_state" | "recording_state" => Kind::Text,
        "score" | "duration" => Kind::Number,
        "started_at" => Kind::Date,
        "has_recording" | "human_connected" => Kind::Boolean,
        "tag_ids" => Kind::StringArray,
        _ => return None,
    })
}
fn scalar_valid(value: &Value, kind: Kind) -> bool {
    match kind {
        Kind::Text => value.as_str().is_some_and(|s| s.len() <= 1000),
        Kind::Number => value.as_f64().is_some_and(f64::is_finite),
        Kind::Date => value
            .as_str()
            .is_some_and(|s| DateTime::parse_from_rfc3339(s).is_ok()),
        Kind::Boolean => value.is_boolean(),
        Kind::StringArray => value.as_array().is_some_and(|a| {
            a.len() <= 100
                && a.iter()
                    .all(|v| v.as_str().is_some_and(|s| s.len() <= 1000))
        }),
    }
}
/// Validate field/operator allowlists, scalar types and bounded AST complexity.
pub fn validate_filter(ast: &Value) -> Result<(), String> {
    fn visit(v: &Value, depth: usize, count: &mut usize) -> Result<(), String> {
        *count += 1;
        if depth > 6 || *count > 64 {
            return Err("filter_too_complex".into());
        }
        let obj = v.as_object().ok_or("invalid_filter_node")?;
        let op = v
            .get("op")
            .and_then(Value::as_str)
            .ok_or("invalid_filter_operator")?;
        if ["and", "or"].contains(&op) {
            if obj.len() != 2 || !obj.contains_key("rules") {
                return Err("invalid_filter_group".into());
            }
            let rules = v["rules"].as_array().ok_or("invalid_filter_rules")?;
            for rule in rules {
                visit(rule, depth + 1, count)?
            }
            return Ok(());
        }
        if op == "not" {
            if obj.len() != 2 {
                return Err("invalid_filter_not".into());
            }
            return visit(v.get("rule").ok_or("invalid_filter_not")?, depth + 1, count);
        }
        if obj
            .keys()
            .any(|k| !["op", "field", "value"].contains(&k.as_str()))
        {
            return Err("unknown_filter_property".into());
        }
        let field = v
            .get("field")
            .and_then(Value::as_str)
            .ok_or("invalid_filter_field")?;
        let kind = field_kind(field).ok_or("unknown_filter_field")?;
        let value = v.get("value");
        if op == "is_null" {
            if value.is_some_and(|x| !x.is_boolean()) {
                return Err("invalid_null_operator".into());
            }
            return Ok(());
        }
        let value = value.ok_or("filter_value_required")?;
        match op {
            "eq" | "ne" => {
                if !scalar_valid(value, kind) {
                    return Err("filter_type_mismatch".into());
                }
            }
            "in" => {
                let values = value.as_array().ok_or("filter_type_mismatch")?;
                let scalar_kind = if kind == Kind::StringArray {
                    Kind::Text
                } else {
                    kind
                };
                if values.is_empty()
                    || values.len() > 100
                    || !values.iter().all(|v| scalar_valid(v, scalar_kind))
                {
                    return Err("filter_type_mismatch".into());
                }
            }
            "contains" => {
                if !matches!(kind, Kind::Text | Kind::StringArray)
                    || !scalar_valid(value, Kind::Text)
                {
                    return Err("filter_type_mismatch".into());
                }
            }
            "gt" | "gte" | "lt" | "lte" => {
                if !matches!(kind, Kind::Number | Kind::Date) || !scalar_valid(value, kind) {
                    return Err("filter_type_mismatch".into());
                }
            }
            _ => return Err("unknown_filter_operator".into()),
        }
        Ok(())
    }
    visit(ast, 1, &mut 0)
}
/// Boolean filter semantics. Unknown numeric scores are never coerced to zero.
pub fn matches_filter(data: &Value, ast: &Value) -> bool {
    fn eval(data: &Value, v: &Value) -> bool {
        let op = v["op"].as_str().unwrap_or("");
        if op == "and" {
            return v["rules"]
                .as_array()
                .is_some_and(|a| a.iter().all(|r| eval(data, r)));
        }
        if op == "or" {
            return v["rules"]
                .as_array()
                .is_some_and(|a| a.iter().any(|r| eval(data, r)));
        }
        if op == "not" {
            return !eval(data, &v["rule"]);
        }
        let field = v["field"].as_str().unwrap_or("");
        let actual = &data[field];
        let expected = &v["value"];
        if op == "is_null" {
            return actual.is_null() == expected.as_bool().unwrap_or(true);
        }
        if actual.is_null() {
            return false;
        }
        match op {
            "eq" => actual == expected,
            "ne" => actual != expected,
            "contains" => match (actual.as_array(), actual.as_str(), expected.as_str()) {
                (Some(values), _, Some(text)) => values.iter().any(|v| v.as_str() == Some(text)),
                (_, Some(text), Some(needle)) => {
                    text.to_lowercase().contains(&needle.to_lowercase())
                }
                _ => false,
            },
            "in" => expected.as_array().is_some_and(|values| {
                if let Some(a) = actual.as_array() {
                    a.iter().any(|v| values.contains(v))
                } else {
                    values.contains(actual)
                }
            }),
            _ => {
                let compare = if field == "started_at" {
                    actual
                        .as_str()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .zip(
                            expected
                                .as_str()
                                .and_then(|s| DateTime::parse_from_rfc3339(s).ok()),
                        )
                        .map(|(a, b)| a.cmp(&b))
                } else {
                    actual
                        .as_f64()
                        .zip(expected.as_f64())
                        .and_then(|(a, b)| a.partial_cmp(&b))
                };
                compare.is_some_and(|cmp| match op {
                    "gt" => cmp.is_gt(),
                    "gte" => !cmp.is_lt(),
                    "lt" => cmp.is_lt(),
                    "lte" => !cmp.is_gt(),
                    _ => false,
                })
            }
        }
    }
    validate_filter(ast).is_ok() && eval(data, ast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nesting_types_and_injection_fields_rejected() {
        assert!(validate_filter(
            &json!({"field":"phone'); DROP TABLE calls;--","op":"eq","value":"x"})
        )
        .is_err());
        assert!(validate_filter(&json!({"field":"score","op":"gt","value":"1"})).is_err());
        let mut f = json!({"op":"and","rules":[]});
        for _ in 0..6 {
            f = json!({"op":"not","rule":f})
        }
        assert!(validate_filter(&f).is_err());
        assert!(validate_filter(&json!({"op":"and","rules":(0..64).map(|_|json!({"op":"and","rules":[]})).collect::<Vec<_>>()})).is_err());
    }
    #[test]
    fn unknown_and_zero_scores_differ() {
        let f = json!({"field":"score","op":"gte","value":0});
        assert!(!matches_filter(&json!({"score":null}), &f));
        assert!(matches_filter(&json!({"score":0}), &f));
        assert!(matches_filter(
            &json!({"score":null}),
            &json!({"field":"score","op":"is_null"})
        ));
    }
    #[test]
    fn recording_availability_filters_are_distinct_from_provider_presence() {
        let filter = json!({"field":"recording_state","op":"eq","value":"ready"});
        assert!(validate_filter(&filter).is_ok());
        assert!(!matches_filter(
            &json!({"has_recording":true,"recording_state":"pending"}),
            &filter
        ));
        assert!(matches_filter(
            &json!({"has_recording":true,"recording_state":"ready"}),
            &filter
        ));
        assert!(
            validate_filter(&json!({"field":"recording_state","op":"eq","value":true})).is_err()
        );
    }
    #[test]
    fn nested_tag_filters_and_timezone_dates() {
        let c = json!({"tag_ids":["a","b"],"employee":"Иван","started_at":"2026-11-11T09:00:00Z"});
        let f = json!({"op":"and","rules":[{"field":"tag_ids","op":"in","value":["b","c"]},{"op":"not","rule":{"field":"employee","op":"eq","value":"Другой"}},{"field":"started_at","op":"gte","value":"2026-11-11T12:00:00+03:00"}]});
        assert!(matches_filter(&c, &f));
    }
}
