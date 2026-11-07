//! Fixed T2 verifier. The surrounding contract authenticates task bindings and
//! payload bytes before this module evaluates their meaning.

use near_sdk::{AccountId, env, json_types::Base64VecU8, near, serde::Deserialize};

use crate::FailureClass;

pub const TASK_TYPE: &str = "t2_fee_split_v1";
pub const MAX_EVENTS: usize = 100;
pub const MAX_ENTRIES: usize = 128;
pub const MAX_SPEC_BYTES: usize = 1_024;
pub const MAX_PAYLOAD_BYTES: usize = 8_192;
pub const MAX_EVIDENCE_BYTES: usize = 32_768;
pub const MAX_CALL_BYTES: usize = 49_152;

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
pub struct TaskSpec {
    task_type: String,
    pub fee_basis_points: u16,
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
struct Ledger {
    entries: Vec<String>,
    declared_total: String,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskLink {
    pub assignment_seq: u64,
    pub worker: AccountId,
    pub parent_assignment_seq: Option<u64>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct EvidenceSnapshot {
    pub event_count: u64,
    pub head_hash: String,
    pub challenge_height: u64,
}

#[near(serializers = [json])]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub seq: u64,
    pub bytes: Base64VecU8,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub enum Verdict {
    Upheld,
    Overturned,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub enum Origin {
    Introduction,
    Propagation,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Finding {
    pub agent: AccountId,
    pub failure_class: FailureClass,
    pub event_seq: u64,
    /// The decisive fault itself is an introduction, not its downstream symptom.
    pub origin: Origin,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Verification {
    pub verdict: Verdict,
    pub finding: Option<Finding>,
    /// The reported check's relationship to the decisive fault. In S6, the
    /// reported worker F1 is propagation, while the decisive F3 is introduction.
    pub origin: Option<Origin>,
    pub assignment_seq: u64,
    pub snapshot: EvidenceSnapshot,
}

#[near(serializers = [json])]
pub struct VerifierInfo {
    pub task_type: String,
    pub verifier_hash: String,
    pub identity_kind: String,
    pub max_events: usize,
    pub max_entries: usize,
    pub max_spec_bytes: usize,
    pub max_payload_bytes: usize,
    pub max_evidence_bytes: usize,
    pub max_call_bytes: usize,
}

pub fn info() -> VerifierInfo {
    VerifierInfo {
        task_type: TASK_TYPE.into(),
        verifier_hash: implementation_hash(),
        identity_kind: "source-and-dependency-manifest-sha256-v1".into(),
        max_events: MAX_EVENTS,
        max_entries: MAX_ENTRIES,
        max_spec_bytes: MAX_SPEC_BYTES,
        max_payload_bytes: MAX_PAYLOAD_BYTES,
        max_evidence_bytes: MAX_EVIDENCE_BYTES,
        max_call_bytes: MAX_CALL_BYTES,
    }
}

/// A versioned source identity, NOT a claim to hash or attest the deployed WASM.
/// Full WASM hashes and their build/source correspondence are recorded separately.
pub fn implementation_hash() -> String {
    let mut manifest = b"wbts/embedded-t2/source-manifest/v1\0".to_vec();
    for source in [
        include_bytes!("lib.rs").as_slice(),
        include_bytes!("t2.rs").as_slice(),
        include_bytes!("../Cargo.toml").as_slice(),
        include_bytes!("../Cargo.lock").as_slice(),
    ] {
        manifest.extend_from_slice(&(source.len() as u64).to_le_bytes());
        manifest.extend_from_slice(source);
    }
    hex::encode(env::sha256(&manifest))
}

pub fn parse_spec(bytes: &[u8]) -> Result<TaskSpec, &'static str> {
    if bytes.len() > MAX_SPEC_BYTES {
        return Err("task spec exceeds byte limit");
    }
    let spec: TaskSpec =
        near_sdk::serde_json::from_slice(bytes).map_err(|_| "invalid task spec")?;
    if spec.task_type != TASK_TYPE || spec.fee_basis_points > 10_000 {
        return Err("unsupported task spec");
    }
    Ok(spec)
}

pub enum Check {
    PreconditionFault,
    PostconditionFault,
    Passed,
}

pub fn verify(spec: &TaskSpec, input: &[u8], output: Option<&[u8]>) -> Result<Check, &'static str> {
    if input.len() > MAX_PAYLOAD_BYTES
        || output.is_some_and(|bytes| bytes.len() > MAX_PAYLOAD_BYTES)
    {
        return Err("payload exceeds byte limit");
    }
    // Input validity precedes output execution. Even a perfectly parseable
    // worker result cannot cure the delegator's malformed input contract.
    let Ok(ledger) = near_sdk::serde_json::from_slice::<Ledger>(input) else {
        return Ok(Check::PreconditionFault);
    };
    if ledger.entries.is_empty() || ledger.entries.len() > MAX_ENTRIES {
        return Ok(Check::PreconditionFault);
    }
    let Some(total) = ledger
        .entries
        .iter()
        .try_fold(0u128, |total, entry| total.checked_add(decimal(entry)?))
    else {
        return Ok(Check::PreconditionFault);
    };
    if decimal(&ledger.declared_total) != Some(total) {
        return Ok(Check::PreconditionFault);
    }
    let output = output.ok_or("missing completion; timeout verification is unsupported")?;
    // floor(total*bps/10000), without overflowing an otherwise valid u128 total.
    // Each partial product and the final sum are bounded by total (bps<=10000).
    let bps = u128::from(spec.fee_basis_points);
    let fee = (total / 10_000)
        .checked_mul(bps)
        .and_then(|base| base.checked_add((total % 10_000) * bps / 10_000))
        .ok_or("fee arithmetic overflow")?;
    let net = total.checked_sub(fee).ok_or("fee exceeds total")?;
    let expected = format!(r#"{{"fee":"{fee}","net":"{net}"}}"#);
    Ok(if output == expected.as_bytes() {
        Check::Passed
    } else {
        Check::PostconditionFault
    })
}

fn decimal(value: &str) -> Option<u128> {
    if value.is_empty()
        || value.len() > 39
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}
