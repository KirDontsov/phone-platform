use chrono::{DateTime, Datelike, Duration, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn human_connected(call: &Value) -> bool {
    call["provider_result"].as_str() == Some("bridged")
        || call["human_connected"].as_bool() == Some(true)
}
fn at(call: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(call["started_at"].as_str()?)
        .ok()
        .map(|v| v.with_timezone(&Utc))
}
fn phone(call: &Value) -> Option<String> {
    let s = call["phone"].as_str()?;
    let digits = s.chars().filter(char::is_ascii_digit).collect::<String>();
    if !(7..=15).contains(&digits.len()) {
        return None;
    }
    Some(digits)
}
fn successful(call: &Value, minimum: i64) -> bool {
    human_connected(call) && call["duration"].as_i64().unwrap_or(0) >= minimum
}

/// Validate user-editable callback settings before saving organization configuration.
pub fn validate_callback_policy(policy: &Value) -> Result<(), String> {
    if let Some(value) = policy.get("timezone") {
        if !value.as_str().is_some_and(|s| s.parse::<Tz>().is_ok()) {
            return Err("invalid_timezone".into());
        }
    }
    if let Some(value) = policy.get("callback_minutes") {
        if !value.as_i64().is_some_and(|v| (1..=43200).contains(&v)) {
            return Err("invalid_callback_minutes".into());
        }
    }
    if let Some(value) = policy.get("callback_min_duration") {
        if !value.as_i64().is_some_and(|v| (0..=3600).contains(&v)) {
            return Err("invalid_callback_min_duration".into());
        }
    }
    if policy
        .get("callback_allow_inbound")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err("invalid_callback_allow_inbound".into());
    }
    if let Some(hours) = policy.get("working_hours").filter(|v| !v.is_null()) {
        if !hours.is_object() || !hours.get("enabled").is_some_and(Value::is_boolean) {
            return Err("invalid_working_hours".into());
        }
        if hours["enabled"] == true {
            let start = hours["start"]
                .as_str()
                .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
                .ok_or("invalid_working_hours_start")?;
            let end = hours["end"]
                .as_str()
                .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
                .ok_or("invalid_working_hours_end")?;
            if start == end {
                return Err("empty_working_interval".into());
            }
            let days = hours["days"].as_array().ok_or("invalid_working_days")?;
            if days.is_empty()
                || days.len() > 7
                || !days
                    .iter()
                    .all(|d| d.as_u64().is_some_and(|d| (1..=7).contains(&d)))
            {
                return Err("invalid_working_days".into());
            }
        }
    }
    Ok(())
}

/// Working-hours deadlines are calculated in UTC minute steps, so DST ambiguities cannot panic.
fn deadline(opened: DateTime<Utc>, minutes: i64, policy: &Value) -> DateTime<Utc> {
    let working = &policy["working_hours"];
    if working.is_null() || working.get("enabled").and_then(Value::as_bool) != Some(true) {
        return opened + Duration::minutes(minutes);
    }
    let timezone = policy["timezone"]
        .as_str()
        .unwrap_or("Europe/Moscow")
        .parse::<Tz>()
        .unwrap_or(chrono_tz::Europe::Moscow);
    let start = working["start"]
        .as_str()
        .and_then(|v| NaiveTime::parse_from_str(v, "%H:%M").ok())
        .unwrap_or(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
    let end = working["end"]
        .as_str()
        .and_then(|v| NaiveTime::parse_from_str(v, "%H:%M").ok())
        .unwrap_or(NaiveTime::from_hms_opt(18, 0, 0).unwrap());
    let days = working["days"]
        .as_array()
        .map(|v| v.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .unwrap_or(vec![1, 2, 3, 4, 5]);
    // Account for a partial opening minute rather than rounding the deadline backwards.
    let mut cursor = opened;
    let mut remaining = minutes.saturating_mul(60);
    let cap = opened + Duration::days(366);
    while remaining > 0 && cursor < cap {
        let local = cursor.with_timezone(&timezone);
        let weekday = u64::from(local.weekday().number_from_monday());
        let time = local.time();
        let active = days.contains(&weekday)
            && if end > start {
                time >= start && time < end
            } else {
                time >= start || time < end
            };
        let step = (60 - i64::from(cursor.second())).min(remaining.max(1));
        if active {
            remaining -= step
        }
        cursor += Duration::seconds(step);
    }
    cursor
}

/// Opens episodes only for external incoming calls without a confirmed human connection.
/// All passed calls must already be tenant-scoped by the repository/service.
pub fn callback_episodes(calls: &[Value], policy: &Value, as_of: DateTime<Utc>) -> Vec<Value> {
    let policy_valid = validate_callback_policy(policy).is_ok();
    let valid_policy = if policy_valid {
        policy.clone()
    } else {
        json!({})
    };
    let policy = &valid_policy;
    let mut ordered = calls
        .iter()
        .filter_map(|c| at(c).filter(|t| *t <= as_of).map(|t| (t, c)))
        .collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1["id"].as_str().cmp(&b.1["id"].as_str()))
    });
    let minutes = policy["callback_minutes"]
        .as_i64()
        .unwrap_or(120)
        .clamp(1, 43200);
    let minimum = policy["callback_min_duration"].as_i64().unwrap_or(1).max(0);
    let allow_inbound = policy_valid && policy["callback_allow_inbound"].as_bool().unwrap_or(true);
    let mut active: BTreeMap<String, usize> = BTreeMap::new();
    let mut episodes: Vec<Value> = vec![];
    for (time, call) in ordered {
        if call["deleted"].as_bool() == Some(true) || call["direction"].as_str() == Some("internal")
        {
            continue;
        }
        let Some(key) = phone(call) else { continue };
        let direction = call["direction"].as_str().unwrap_or("");
        let connected = successful(call, minimum);
        if let Some(&idx) = active.get(&key) {
            if connected && (direction == "out" || (direction == "in" && allow_inbound)) {
                episodes[idx]["state"] = json!("resolved");
                episodes[idx]["resolved_at"] = json!(time);
                episodes[idx]["resolved_by"] = call["id"].clone();
                episodes[idx]["reason"] = json!("human_connected");
                active.remove(&key);
                continue;
            }
            if direction == "out" {
                let count = episodes[idx]["attempts"].as_i64().unwrap_or(0) + 1;
                episodes[idx]["attempts"] = json!(count);
                if call["coverage_complete"].as_bool() != Some(true) {
                    episodes[idx]["coverage_complete"] = json!(false)
                }
            }
            if direction == "in" && !connected {
                let count = episodes[idx]["missed_count"].as_i64().unwrap_or(1) + 1;
                episodes[idx]["missed_count"] = json!(count);
                if call["coverage_complete"].as_bool() != Some(true) {
                    episodes[idx]["coverage_complete"] = json!(false)
                }
            }
            continue;
        }
        if direction == "in" && !connected {
            let due = deadline(time, minutes, policy);
            let episode = json!({"call_id":call["id"],"phone":call["phone"],"employee":call["employee"],"opened_at":time,"due_at":due,"attempts":0,"missed_count":1,"state":"open","reason":"no_confirmed_human_contact","coverage_complete":call["coverage_complete"].as_bool().unwrap_or(false)});
            active.insert(key, episodes.len());
            episodes.push(episode);
        }
    }
    for idx in active.values() {
        let complete = policy_valid && episodes[*idx]["coverage_complete"].as_bool() == Some(true);
        let due = DateTime::parse_from_rfc3339(episodes[*idx]["due_at"].as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        episodes[*idx]["state"] = json!(if !complete {
            "uncertain"
        } else if as_of >= due {
            "overdue"
        } else {
            "open"
        });
        if !complete {
            episodes[*idx]["reason"] = json!(if policy_valid {
                "history_coverage_incomplete"
            } else {
                "invalid_callback_policy"
            })
        }
    }
    episodes
}

pub fn contact_kind(history: &[Value], call: &Value) -> String {
    let Some(key) = phone(call) else {
        return "unknown".into();
    };
    let Some(time) = at(call) else {
        return "unknown".into();
    };
    let earlier = history.iter().any(|c| {
        c["id"] != call["id"]
            && c["deleted"].as_bool() != Some(true)
            && phone(c).as_deref() == Some(&key)
            && at(c).is_some_and(|t| t < time)
    });
    if earlier {
        "repeat"
    } else if call["coverage_complete"].as_bool() == Some(true) {
        "primary"
    } else {
        "unknown"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(id: &str, time: &str, direction: &str, result: &str, coverage: bool) -> Value {
        json!({"id":id,"started_at":time,"phone":"+79990000001","employee":"Сотрудник","direction":direction,"provider_result":result,"duration":30,"coverage_complete":coverage})
    }
    fn date(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    #[test]
    fn ivr_answered_never_resolves_but_bridged_does() {
        let first = call("a", "2026-11-11T09:00:00Z", "in", "not answered", true);
        let ivr = call("b", "2026-11-11T10:00:00Z", "in", "answered", true);
        let now = date("2026-11-11T12:00:00Z");
        let mut calls = vec![first, ivr];
        let es = callback_episodes(&calls, &json!({"callback_minutes":120}), now);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0]["state"], "overdue");
        assert_eq!(es[0]["missed_count"], 2);
        calls.push(call("c", "2026-11-11T11:00:00Z", "out", "bridged", true));
        assert_eq!(
            callback_episodes(&calls, &json!({}), now)[0]["state"],
            "resolved"
        );
    }
    #[test]
    fn failed_attempt_counts_and_unknown_is_not_definitive() {
        let calls = vec![
            call("a", "2026-11-11T09:00:00Z", "in", "not answered", false),
            call("b", "2026-11-11T10:00:00Z", "out", "not answered", true),
        ];
        let es = callback_episodes(&calls, &json!({}), date("2026-11-11T15:00:00Z"));
        assert_eq!(es[0]["attempts"], 1);
        assert_eq!(es[0]["state"], "uncertain");
    }
    #[test]
    fn no_new_call_is_required_for_overdue() {
        let calls = vec![call("a", "2026-11-11T09:00:00Z", "in", "rejected", true)];
        assert_eq!(
            callback_episodes(&calls, &json!({}), date("2026-11-11T10:00:00Z"))[0]["state"],
            "open"
        );
        assert_eq!(
            callback_episodes(&calls, &json!({}), date("2026-11-11T12:00:00Z"))[0]["state"],
            "overdue"
        );
    }
    #[test]
    fn weekend_deadline_and_primary_coverage() {
        let first = call("a", "2026-11-13T14:30:00Z", "in", "not answered", true);
        let policy = json!({"callback_minutes":60,"timezone":"Europe/Moscow","working_hours":{"enabled":true,"start":"09:00","end":"18:00","days":[1,2,3,4,5]}});
        let es = callback_episodes(&[first.clone()], &policy, date("2026-11-16T08:00:00Z"));
        assert_eq!(es[0]["due_at"], "2026-11-16T06:30:00Z");
        assert_eq!(contact_kind(&[], &first), "primary");
        let mut incomplete = first;
        incomplete["coverage_complete"] = json!(false);
        assert_eq!(contact_kind(&[], &incomplete), "unknown");
    }
    #[test]
    fn invalid_calendar_is_rejected_and_dst_uses_real_elapsed_work_minutes() {
        assert!(validate_callback_policy(&json!({"timezone":"not/a-zone"})).is_err());
        assert!(validate_callback_policy(
            &json!({"working_hours":{"enabled":true,"start":"09:00","end":"18:00","days":[8]}})
        )
        .is_err());
        let c = call("dst", "2026-11-01T05:30:00Z", "in", "not answered", true);
        let policy = json!({"callback_minutes":60,"timezone":"America/New_York","working_hours":{"enabled":true,"start":"01:00","end":"03:00","days":[7]}});
        let rows = callback_episodes(&[c], &policy, date("2026-11-01T08:00:00Z"));
        assert_eq!(rows[0]["due_at"], "2026-11-01T06:30:00Z");
    }
}
