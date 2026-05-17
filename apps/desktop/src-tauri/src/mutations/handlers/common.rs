use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Context, Result};

use crate::mutations::engine::MutationApplyError;
use crate::mutations::patch::PatchValue;
use crate::mutations::{IdMappingDraft, Patch, PredictedEffect, ServerCallShape};

pub fn required_str<'a>(input: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    input
        .get(key)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("mutation input missing string `{key}`"))
}

pub fn optional_str<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(serde_json::Value::as_str)
}

pub fn required_i64(input: &serde_json::Value, key: &str) -> Result<i64> {
    input
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .with_context(|| format!("mutation input missing integer `{key}`"))
}

pub fn optional_i64(input: &serde_json::Value, key: &str) -> Option<i64> {
    input.get(key).and_then(serde_json::Value::as_i64)
}

pub fn string_vec(input: &serde_json::Value, key: &str) -> Result<Vec<String>> {
    let values = input
        .get(key)
        .and_then(serde_json::Value::as_array)
        .with_context(|| format!("mutation input missing array `{key}`"))?;
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(
            value
                .as_str()
                .map(ToString::to_string)
                .ok_or_else(|| anyhow!("mutation input `{key}` must contain strings"))?,
        );
    }
    Ok(out)
}

pub fn deduped_string_set(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn now_epoch_seconds() -> Result<i64> {
    super::super::reconciler::now_epoch_seconds()
}

pub fn mutation_state_json(mutation_id: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "mutation_id": mutation_id,
        "kind": kind,
    })
}

pub fn predicted_effect(
    forward_patch: Patch,
    server_call: ServerCallShape,
    id_mappings: Vec<IdMappingDraft>,
) -> PredictedEffect {
    PredictedEffect {
        inverse_patch: forward_patch.inverse(),
        forward_patch,
        id_mappings,
        server_call,
    }
}

pub fn no_op_effect(server_call: ServerCallShape) -> PredictedEffect {
    PredictedEffect {
        forward_patch: Patch::empty(),
        inverse_patch: Patch::empty(),
        id_mappings: Vec::new(),
        server_call,
    }
}

pub fn single_pk(column: &str, value: PatchValue) -> BTreeMap<String, PatchValue> {
    BTreeMap::from([(column.to_string(), value)])
}

pub fn map_apply_error(error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    if let Some(status) = parse_http_status(&message) {
        return MutationApplyError::http(status, message).into();
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("connection")
        || lower.contains("timed out")
        || lower.contains("dns")
        || lower.contains("network")
    {
        return MutationApplyError::network(message).into();
    }
    error
}

fn parse_http_status(message: &str) -> Option<i64> {
    let mut digits = String::new();
    let mut in_parens = false;
    for ch in message.chars() {
        if ch == '(' {
            in_parens = true;
            digits.clear();
            continue;
        }
        if ch == ')' && in_parens {
            if (3..=4).contains(&digits.len()) {
                return digits.parse::<i64>().ok();
            }
            in_parens = false;
            continue;
        }
        if in_parens {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else {
                in_parens = false;
                digits.clear();
            }
        }
    }
    None
}
