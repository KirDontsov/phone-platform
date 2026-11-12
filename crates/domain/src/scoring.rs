use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 20000)
        .ok_or_else(|| format!("invalid_{key}"))
}
fn string_array(value: &Value, key: &str) -> Result<(), String> {
    let a = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("invalid_{key}"))?;
    if a.len() > 100
        || !a.iter().all(|s| {
            s.as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 10000)
        })
    {
        return Err(format!("invalid_{key}"));
    }
    Ok(())
}

/// Enforces evidence and trusted rubric/taxonomy; computes overall score itself.
/// Does not apply suggestions to calls. That separate service must preserve manual overrides.
pub fn validate_analysis(
    result: &mut Value,
    transcript: &Value,
    rubric: &Value,
    allowed_statuses: &[String],
    allowed_tags: &[String],
) -> Result<(), String> {
    let object = result.as_object().ok_or("analysis_must_be_object")?;
    let allowed = [
        "id",
        "state",
        "model",
        "provider",
        "prompt_version",
        "rubric_version",
        "transcript_id",
        "summary",
        "customer_intent",
        "agreements",
        "next_steps",
        "suggested_status_id",
        "suggested_tag_ids",
        "criteria",
        "score",
        "abstention_reason",
        "created_at",
        "usage",
    ];
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown_analysis_property".into());
    }
    string(result, "summary")?;
    string(result, "customer_intent")?;
    string_array(result, "agreements")?;
    string_array(result, "next_steps")?;
    let status = result
        .get("suggested_status_id")
        .ok_or("suggested_status_required")?;
    if !status.is_null()
        && !status
            .as_str()
            .is_some_and(|id| allowed_statuses.iter().any(|s| s == id))
    {
        return Err("unknown_suggested_status".into());
    }
    let tags = result
        .get("suggested_tag_ids")
        .and_then(Value::as_array)
        .ok_or("suggested_tags_required")?;
    let mut seen_tags = HashSet::new();
    if tags.len() > 30
        || tags.iter().any(|tag| {
            !tag.as_str()
                .is_some_and(|id| allowed_tags.iter().any(|s| s == id) && seen_tags.insert(id))
        })
    {
        return Err("unknown_suggested_tag".into());
    }
    let segments = transcript
        .get("segments")
        .and_then(Value::as_array)
        .ok_or("transcript_segments_required")?;
    let mut segment_map = HashMap::new();
    for segment in segments {
        let id = string(segment, "id")?;
        let start = segment["start"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or("invalid_segment_time")?;
        let end = segment["end"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= start)
            .ok_or("invalid_segment_time")?;
        string(segment, "text")?;
        if segment_map.insert(id, (segment, start, end)).is_some() {
            return Err("duplicate_segment_id".into());
        }
    }
    let definitions = rubric
        .get("criteria")
        .and_then(Value::as_array)
        .ok_or("rubric_criteria_required")?;
    if definitions.is_empty() || definitions.len() > 100 {
        return Err("invalid_rubric_size".into());
    }
    let mut definition_map = HashMap::new();
    for definition in definitions {
        let id = string(definition, "id")?;
        string(definition, "name")?;
        let weight = definition["weight"]
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0. && *v <= 1000.)
            .ok_or("invalid_rubric_weight")?;
        if definition
            .get("evaluation_mode")
            .is_some_and(|v| !matches!(v.as_str(), Some("score" | "checklist")))
        {
            return Err("invalid_evaluation_mode".into());
        }
        if definition_map.insert(id, (definition, weight)).is_some() {
            return Err("duplicate_rubric_criterion".into());
        }
    }
    let criteria = result
        .get_mut("criteria")
        .and_then(Value::as_array_mut)
        .ok_or("analysis_criteria_required")?;
    if criteria.len() != definitions.len() {
        return Err("criterion_coverage_mismatch".into());
    }
    let mut seen = HashSet::new();
    let mut weighted = 0.;
    let mut total_weight = 0.;
    let mut insufficient = false;
    let roles_known = transcript["role_confidence"].as_str() == Some("known");
    for criterion in criteria {
        let allowed = [
            "id",
            "name",
            "weight",
            "score",
            "status",
            "reason",
            "evidence",
            "evaluation_mode",
            "fulfilled",
        ];
        if criterion
            .as_object()
            .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
        {
            return Err("unknown_criterion_property".into());
        }
        let id = string(criterion, "id")?.to_owned();
        if !seen.insert(id.clone()) {
            return Err("duplicate_analysis_criterion".into());
        }
        let (definition, weight) = definition_map
            .get(id.as_str())
            .ok_or("unknown_analysis_criterion")?;
        let mut status = string(criterion, "status")?.to_owned();
        if !["scored", "not_applicable", "insufficient_evidence"].contains(&status.as_str()) {
            return Err("invalid_criterion_status".into());
        }
        string(criterion, "reason")?;
        let evidence = criterion
            .get("evidence")
            .and_then(Value::as_array)
            .ok_or("criterion_evidence_required")?;
        if evidence.len() > 30 || (status == "scored" && evidence.is_empty()) {
            return Err("criterion_evidence_required".into());
        }
        for item in evidence {
            let segment_id = string(item, "segment_id")?;
            let (segment, segment_start, segment_end) = segment_map
                .get(segment_id)
                .ok_or("unknown_evidence_segment")?;
            let start = item["start"]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid_evidence_time")?;
            let end = item["end"]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid_evidence_time")?;
            let quote = string(item, "quote")?;
            if start + 0.001 < *segment_start
                || end > *segment_end + 0.001
                || end < start
                || !segment["text"].as_str().unwrap().contains(quote)
            {
                return Err("evidence_not_in_transcript".into());
            }
        }
        let mode = definition["evaluation_mode"].as_str().unwrap_or("score");
        if mode == "checklist" {
            if status == "scored" {
                let fulfilled = criterion["fulfilled"]
                    .as_bool()
                    .ok_or("checklist_verdict_required")?;
                let score = if fulfilled { 5. } else { 0. };
                if criterion["score"].as_f64() != Some(score) {
                    return Err("checklist_score_mismatch".into());
                }
                if !roles_known {
                    status = "insufficient_evidence".into();
                    criterion["status"] = json!(status);
                    criterion["score"] = Value::Null;
                    criterion["fulfilled"] = Value::Null;
                    criterion["reason"] = json!(
                        "Роли участников не подтверждены; выполнение требования не определено."
                    );
                }
            } else if !criterion.get("fulfilled").is_some_and(Value::is_null) {
                return Err("unscored_checklist_has_verdict".into());
            }
        } else {
            // Scaled historical criteria have no trustworthy binary verdict.
            criterion["fulfilled"] = Value::Null;
        }
        criterion["evaluation_mode"] = json!(mode);
        if status == "scored" {
            let score = criterion["score"]
                .as_f64()
                .filter(|s| s.is_finite() && (0. ..=5.).contains(s))
                .ok_or("invalid_criterion_score")?;
            weighted += weight * score / 5.;
            total_weight += weight;
        } else {
            if !criterion.get("score").is_some_and(Value::is_null) {
                return Err("unscored_criterion_has_score".into());
            }
            if definition["required"].as_bool().unwrap_or(true) {
                insufficient = true
            }
        }
        criterion["name"] = definition["name"].clone();
        criterion["weight"] = json!(weight);
    }
    let score = if !insufficient && roles_known && total_weight > 0. {
        Some((weighted / total_weight * 1000.).round() / 10.)
    } else {
        None
    };
    result["score"] = json!(score);
    if !roles_known {
        result["abstention_reason"] = json!("speaker_roles_uncertain")
    } else if insufficient || total_weight == 0. {
        result["abstention_reason"] = json!("insufficient_required_evidence")
    } else {
        result["abstention_reason"] = Value::Null
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn transcript() -> Value {
        json!({"role_confidence":"known","segments":[{"id":"s1","start":0,"end":10,"text":"Здравствуйте, меня зовут Анна."}]})
    }
    fn rubric() -> Value {
        json!({"criteria":[{"id":"greet","name":"Приветствие","weight":2,"required":true}]})
    }
    fn analysis() -> Value {
        json!({"summary":"Клиент получил консультацию","customer_intent":"Консультация","agreements":[],"next_steps":[],"suggested_status_id":null,"suggested_tag_ids":[],"criteria":[{"id":"greet","name":"Модель не управляет этим полем","weight":999,"score":4,"status":"scored","evidence":[{"segment_id":"s1","start":0,"end":10,"quote":"Здравствуйте"}],"reason":"Есть приветствие"}],"score":100})
    }
    #[test]
    fn score_and_weight_are_computed_from_rubric() {
        let mut a = analysis();
        validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).unwrap();
        assert_eq!(a["score"], 80.);
        assert_eq!(a["criteria"][0]["weight"], 2.);
        assert_eq!(a["criteria"][0]["name"], "Приветствие");
    }
    #[test]
    fn hallucinated_evidence_and_taxonomy_fail() {
        let mut a = analysis();
        a["criteria"][0]["evidence"][0]["segment_id"] = json!("fake");
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).is_err());
        let mut a = analysis();
        a["suggested_tag_ids"] = json!(["foreign-tenant-tag"]);
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).is_err());
        let mut a = analysis();
        a["criteria"][0]["evidence"][0]["quote"] = json!("Не произнесённая фраза");
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).is_err());
    }
    #[test]
    fn unknown_roles_and_missing_coverage_do_not_become_zero() {
        let mut a = analysis();
        let mut t = transcript();
        t["role_confidence"] = json!("uncertain");
        validate_analysis(&mut a, &t, &rubric(), &[], &[]).unwrap();
        assert!(a["score"].is_null());
        assert_eq!(a["abstention_reason"], "speaker_roles_uncertain");
        let mut a = analysis();
        a["criteria"][0]["status"] = json!("insufficient_evidence");
        a["criteria"][0]["score"] = Value::Null;
        a["criteria"][0]["evidence"] = json!([]);
        validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).unwrap();
        assert!(a["score"].is_null());
    }
    #[test]
    fn suggested_tags_match_manual_limit_and_are_unique() {
        let allowed = (0..31).map(|n| format!("tag-{n}")).collect::<Vec<_>>();
        let mut a = analysis();
        a["suggested_tag_ids"] = json!(&allowed[..30]);
        validate_analysis(&mut a, &transcript(), &rubric(), &[], &allowed).unwrap();
        let mut a = analysis();
        a["suggested_tag_ids"] = json!(allowed);
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &allowed).is_err());
        let mut a = analysis();
        a["suggested_tag_ids"] = json!(["tag-0", "tag-0"]);
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &allowed).is_err());
    }
    #[test]
    fn duplicate_criteria_and_out_of_range_score_fail() {
        let mut a = analysis();
        a["criteria"][0]["score"] = json!(6);
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).is_err());
        let mut a = analysis();
        let duplicate = a["criteria"][0].clone();
        a["criteria"].as_array_mut().unwrap().push(duplicate);
        assert!(validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).is_err());
    }

    #[test]
    fn checklist_verdict_is_explicit_and_weighted_without_guessing_from_score() {
        let mut r = rubric();
        r["criteria"][0]["evaluation_mode"] = json!("checklist");
        for (fulfilled, score) in [(true, 5), (false, 0)] {
            let mut a = analysis();
            a["criteria"][0]["fulfilled"] = json!(fulfilled);
            a["criteria"][0]["score"] = json!(score);
            validate_analysis(&mut a, &transcript(), &r, &[], &[]).unwrap();
            assert_eq!(a["criteria"][0]["evaluation_mode"], "checklist");
            assert_eq!(a["criteria"][0]["fulfilled"], fulfilled);
            assert_eq!(a["score"], (score * 20) as f64);
        }
        for fulfilled in [Value::Null, json!("yes"), json!(1)] {
            let mut a = analysis();
            a["criteria"][0]["fulfilled"] = fulfilled;
            assert!(validate_analysis(&mut a, &transcript(), &r, &[], &[]).is_err());
        }
        let mut missing = analysis();
        missing["criteria"][0]["score"] = json!(5);
        assert!(validate_analysis(&mut missing, &transcript(), &r, &[], &[]).is_err());
        let mut contradictory = analysis();
        contradictory["criteria"][0]["fulfilled"] = json!(false);
        contradictory["criteria"][0]["score"] = json!(5);
        assert!(validate_analysis(&mut contradictory, &transcript(), &r, &[], &[]).is_err());
    }

    #[test]
    fn checklist_unknown_is_not_false_and_unknown_roles_cannot_produce_a_verdict() {
        let mut r = rubric();
        r["criteria"][0]["evaluation_mode"] = json!("checklist");
        let mut a = analysis();
        a["criteria"][0]["status"] = json!("insufficient_evidence");
        a["criteria"][0]["fulfilled"] = Value::Null;
        a["criteria"][0]["score"] = Value::Null;
        a["criteria"][0]["evidence"] = json!([]);
        validate_analysis(&mut a, &transcript(), &r, &[], &[]).unwrap();
        assert!(a["score"].is_null());
        assert!(a["criteria"][0]["fulfilled"].is_null());
        a["criteria"][0]["fulfilled"] = json!(false);
        assert!(validate_analysis(&mut a, &transcript(), &r, &[], &[]).is_err());

        let mut a = analysis();
        a["criteria"][0]["score"] = json!(5);
        a["criteria"][0]["fulfilled"] = json!(true);
        let mut t = transcript();
        t["role_confidence"] = json!("uncertain");
        validate_analysis(&mut a, &t, &r, &[], &[]).unwrap();
        assert_eq!(a["criteria"][0]["status"], "insufficient_evidence");
        assert!(a["criteria"][0]["fulfilled"].is_null());
        assert!(a["criteria"][0]["score"].is_null());
    }

    #[test]
    fn legacy_scales_do_not_acquire_checklist_verdicts_and_unknown_modes_fail() {
        let mut a = analysis();
        a["criteria"][0]["fulfilled"] = json!(true);
        validate_analysis(&mut a, &transcript(), &rubric(), &[], &[]).unwrap();
        assert!(a["criteria"][0]["fulfilled"].is_null());
        assert_eq!(a["criteria"][0]["evaluation_mode"], "score");
        assert_eq!(a["score"], 80.);
        let mut r = rubric();
        r["criteria"][0]["evaluation_mode"] = json!("invented");
        assert!(validate_analysis(&mut analysis(), &transcript(), &r, &[], &[]).is_err());
    }
}
