use clap::Parser;
use serde_json::json;
use wbts_cli::{App, NEAR, canonical, verify_money};

#[test]
fn canonical_payload_hash_input_is_stable_for_nested_objects() {
    assert_eq!(
        canonical(br#"{ "z": [{"b": 2, "a": 1}], "a": "x" }"#).unwrap(),
        br#"{"a":"x","z":[{"a":1,"b":2}]}"#
    );
    assert!(canonical(b"not json").is_err());
}

#[test]
fn account_and_storage_validation_reject_mainnet_and_unbounded_allowance() {
    for args in [
        vec!["cli", "--contract", "example.near", "run-demo", "s2"],
        vec![
            "cli",
            "--storage-allowance",
            "100000000000000000000001",
            "run-demo",
            "s2",
        ],
    ] {
        assert!(App::parse_from(args).validate().is_err());
    }
}

#[test]
fn reputation_floor_and_cap_and_balance_mismatch_are_checked() {
    let agents = vec![
        "reporter.testnet".into(),
        "worker.testnet".into(),
        "challenger.testnet".into(),
    ];
    let before = json!({"accounts":{
        "reporter.testnet":{"amount":NEAR.to_string()},"worker.testnet":{"amount":NEAR.to_string()},"challenger.testnet":{"amount":NEAR.to_string()}},
        "reputations":{"reporter.testnet":7,"worker.testnet":119,"challenger.testnet":120},"treasury":"0"});
    let mut after = json!({"accounts":{
        "reporter.testnet":{"amount":(NEAR*3/4).to_string()},"worker.testnet":{"amount":NEAR.to_string()},"challenger.testnet":{"amount":(NEAR*6/5).to_string()}},
        "reputations":{"reporter.testnet":0,"worker.testnet":120,"challenger.testnet":120},"treasury":(NEAR/20).to_string()});
    assert!(verify_money(&before, &after, &agents, &[]).is_ok());
    after["accounts"]["challenger.testnet"]["amount"] = json!((NEAR * 6 / 5 + 1).to_string());
    assert!(verify_money(&before, &after, &agents, &[]).is_err());
}
