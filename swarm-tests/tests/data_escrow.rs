#[path = "common/data.rs"]
mod data;
use data::*;
use near_contract_sim::{Balance, Fault, Gas, Receipts};
use serde_json::json;

#[test]
fn seller_is_paid_without_a_buyer_acceptance_call_and_wallets_reconcile() {
    let mut r = DataRun::new();
    let id = r.open(&["outpatient-mri", "mri-copay"], N / 50);
    r.accept(id);
    r.deliver(id, 1, "mri-copay").assert_success();
    r.deliver(id, 0, "outpatient-mri").assert_success();
    let job = r.job(id);
    assert_eq!(job["close_reason"], "Fulfilled");
    assert_eq!(num(&job["paid"]), N / 50);
    assert_eq!(num(&job["remaining"]), 0);
    for a in [BUYER, SELLER] {
        let out = r.withdraw_all(a);
        out.assert_success();
        assert!(out.json::<bool>());
        assert_eq!(r.credit(a)["available"], "0");
    }
    r.assert_conserved();
    r.assert_wallets(N / 50);
    assert_eq!(r.accounting()["locked"], "0");
    assert_eq!(r.accounting()["available"], "0");
}

#[test]
fn forged_values_wrong_keys_and_unauthorized_deliveries_move_no_payment() {
    let mut r = DataRun::new();
    let id = r.open(&["mri-copay"], N / 50);
    r.accept(id);
    let before = r.job(id);
    let mut opening = r.opening("mri-copay");
    opening["entry"]["value"] = json!("10");
    r.call(
        SELLER,
        "deliver",
        json!({"job_id":id,"index":0,"opening":opening}),
        ALLOWANCE,
    )
    .assert_panic_contains("proof does not open");
    r.deliver(id, 0, "outpatient-mri")
        .assert_panic_contains("entry does not match");
    r.call(
        OTHER,
        "deliver",
        json!({"job_id":id,"index":0,"opening":r.opening("mri-copay")}),
        ALLOWANCE,
    )
    .assert_panic_contains("only named seller");
    assert_eq!(r.job(id), before);
    assert_eq!(
        r.view("get_delivery", json!({"job_id":id,"index":0})),
        json!(null)
    );
    r.deliver(id, 0, "mri-copay").assert_success();
    r.deliver(id, 0, "mri-copay")
        .assert_panic_contains("job is closed");
    r.assert_conserved();
}

#[test]
fn partial_delivery_expires_to_exact_partial_payout_and_refund() {
    let mut r = DataRun::new();
    let id = r.open(&["outpatient-mri", "mri-copay"], N / 50);
    r.accept(id);
    r.deliver(id, 0, "outpatient-mri").assert_success();
    r.call(BUYER, "cancel_job", json!({"job_id":id}), 0)
        .assert_panic_contains("accepted work cannot be cancelled");
    r.call(OTHER, "expire_job", json!({"job_id":id}), 0)
        .assert_panic_contains("work deadline still open");
    r.expire(id);
    let job = r.job(id);
    assert_eq!(num(&job["paid"]), N / 100);
    assert_eq!(num(&job["refunded"]), N / 100);
    for a in [BUYER, SELLER] {
        r.withdraw_all(a).assert_success();
    }
    r.assert_conserved();
    r.assert_wallets(N / 100);
    let credit = r.credit(BUYER);
    r.call(OTHER, "expire_job", json!({"job_id":id}), 0)
        .assert_panic_contains("job is closed");
    assert_eq!(r.credit(BUYER), credit);
}

#[test]
fn seller_absence_and_unaccepted_offer_allow_permissionless_full_refund() {
    for accept in [false, true] {
        let mut r = DataRun::new();
        let id = r.open(&["mri-copay"], N / 50);
        if accept {
            r.accept(id);
        }
        r.expire(id);
        assert_eq!(r.job(id)["paid"], "0");
        assert_eq!(num(&r.job(id)["refunded"]), N / 50);
        for a in [BUYER, SELLER] {
            r.withdraw_all(a).assert_success();
        }
        r.assert_conserved();
        r.assert_wallets(0);
    }
}

#[test]
fn upstream_unicode_and_vacancy_proofs_have_identical_rust_meaning() {
    let mut r = DataRun::new();
    let id = r.open(&["unicode-fixture", "missing-fact"], 101);
    r.accept(id);
    r.deliver(id, 0, "unicode-fixture").assert_success();
    let delivery = r.deliver(id, 1, "missing-fact");
    delivery.assert_success();
    assert_eq!(delivery.json::<serde_json::Value>()["present"], false);
    let job = r.job(id);
    assert_eq!(job["close_reason"], "ResolvedWithAbsence");
    assert_eq!(job["paid"], "51");
    assert_eq!(job["refunded"], "50");
    r.assert_conserved();
}

#[test]
fn rounding_is_exact_for_every_delivery_order_including_all_eight_keys() {
    for price in [1, 2, 7, 8, 9, 101] {
        let mut r = DataRun::new();
        let keys = [
            "outpatient-mri",
            "cosmetic-surgery",
            "mri-copay",
            "unicode-fixture",
            "extra-0",
            "extra-1",
            "extra-2",
            "extra-3",
        ];
        let id = r.open(&keys, price);
        r.accept(id);
        for i in (0..8).rev() {
            r.deliver(id, i as u8, keys[i]).assert_success();
        }
        assert_eq!(num(&r.job(id)["paid"]), price);
        assert_eq!(r.accounting()["locked"], "0");
        r.assert_conserved();
    }
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2]] {
        let mut r = DataRun::new();
        let keys = ["outpatient-mri", "cosmetic-surgery", "mri-copay"];
        let id = r.open(&keys, 101);
        r.accept(id);
        for i in order {
            r.deliver(id, i as u8, keys[i]).assert_success();
        }
        assert_eq!(r.job(id)["paid"], "101");
        r.assert_conserved();
    }
}

#[test]
fn underfunding_rolls_back_dataset_jobs_and_proof_allocation() {
    let mut r = DataRun::empty();
    r.call(
        SELLER,
        "publish_dataset",
        json!({"name":"sample-v1","root":r.fixture["root"],"source_uri":""}),
        0,
    )
    .assert_panic_contains("insufficient price and storage");
    assert_eq!(
        r.view("get_dataset", json!({"seller":SELLER,"name":"sample-v1"})),
        json!(null)
    );
    r.call(
        SELLER,
        "publish_dataset",
        json!({"name":"sample-v1","root":r.fixture["root"],"source_uri":""}),
        ALLOWANCE,
    )
    .assert_success();
    let h = r.sim.height();
    r.call(BUYER,"open_job",json!({"seller":SELLER,"dataset":"sample-v1","keys":["mri-copay"],"price":(N/50).to_string(),"accept_deadline":h+50,"work_deadline":h+150}),N/50).assert_panic_contains("insufficient price and storage");
    assert_eq!(r.view("get_job_count", json!({})), 0);
    let id = r.open(&["mri-copay"], N / 50);
    r.accept(id);
    let before = r.job(id);
    r.call(
        SELLER,
        "deliver",
        json!({"job_id":id,"index":0,"opening":r.opening("mri-copay")}),
        0,
    )
    .assert_panic_contains("insufficient price and storage");
    assert_eq!(r.job(id), before);
    r.assert_conserved();
}

#[test]
fn published_roots_are_immutable_and_jobs_pin_seller_identity() {
    let mut r = DataRun::new();
    r.call(
        SELLER,
        "publish_dataset",
        json!({"name":"sample-v1","root":"00".repeat(32),"source_uri":""}),
        ALLOWANCE,
    )
    .assert_panic_contains("already published");
    r.call(
        OTHER,
        "publish_dataset",
        json!({"name":"sample-v1","root":"00".repeat(32),"source_uri":""}),
        ALLOWANCE,
    )
    .assert_success();
    let id = r.open(&["mri-copay"], N / 50);
    assert_eq!(r.job(id)["root"], r.fixture["root"]);
    r.call(BUYER, "accept_job", json!({"job_id":id}), 0)
        .assert_panic_contains("only named seller");
    r.call(OTHER, "cancel_job", json!({"job_id":id}), 0)
        .assert_panic_contains("only buyer");
    r.call(OTHER, "close_job", json!({"job_id":id}), 0)
        .assert_panic_contains("only seller");
    r.call(BUYER, "cancel_job", json!({"job_id":id}), 0)
        .assert_success();
    r.assert_conserved();
}

#[test]
fn private_nonce_checked_withdrawals_reject_overlap_and_clear_credit() {
    let mut r = DataRun::new();
    let amount = num(&r.credit(SELLER)["available"]);
    r.call(
        OTHER,
        "on_withdraw",
        json!({"account_id":SELLER,"nonce":1,"amount":amount.to_string()}),
        0,
    )
    .assert_panic_contains("private");
    let tx = r
        .sim
        .call(SELLER, CONTRACT, "withdraw")
        .args_json(json!({"amount":amount.to_string()}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::action("Transfer"), Fault::Hold(20))
        .submit();
    r.sim.step();
    r.sim.step();
    assert_eq!(num(&r.credit(SELLER)["in_flight"]), amount);
    r.call(SELLER, "withdraw", json!({"amount":null}), 1)
        .assert_panic_contains("already pending");
    r.sim.settle();
    r.sim.outcome(tx).assert_success();
    assert_eq!(r.credit(SELLER)["in_flight"], "0");
    r.withdraw_all(SELLER).assert_success();
    assert_eq!(r.credit(SELLER)["available"], "0");
    r.assert_conserved();
}

#[test]
fn failed_transfer_restores_credit_and_permits_safe_retry() {
    use near_contract_sim::nearcore::near_primitives::action::DeleteAccountAction;
    let mut r = DataRun::new();
    let amount = num(&r.credit(SELLER)["available"]);
    let tx = r
        .sim
        .call(SELLER, CONTRACT, "withdraw")
        .args_json(json!({"amount":null}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::action("Transfer"), Fault::Hold(20))
        .submit();
    r.sim.step();
    r.sim.step();
    r.sim
        .tx(SELLER, SELLER)
        .action(DeleteAccountAction {
            beneficiary_id: OTHER.parse().unwrap(),
        })
        .transact()
        .assert_success();
    r.sim.settle();
    let out = r.sim.outcome(tx);
    out.assert_success();
    assert!(!out.json::<bool>());
    assert_eq!(num(&r.credit(SELLER)["available"]), amount + 1);
    assert_eq!(r.credit(SELLER)["in_flight"], "0");
    r.sim.create_account(SELLER, Balance::from_near(10));
    let out = r.withdraw_all(SELLER);
    out.assert_success();
    assert!(out.json::<bool>());
    r.assert_conserved();
}

#[test]
fn failed_callback_leaves_ambiguous_pending_transfer_and_never_blindly_retries() {
    let mut r = DataRun::new();
    let out = r
        .sim
        .call(SELLER, CONTRACT, "withdraw")
        .args_json(json!({"amount":null}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::method("on_withdraw"), Fault::OutOfGas)
        .transact();
    assert!(out.receipts.iter().any(|x| x.failure().is_some()));
    assert!(num(&r.credit(SELLER)["in_flight"]) > 0);
    r.call(SELLER, "withdraw", json!({"amount":null}), 1)
        .assert_panic_contains("already pending");
    r.assert_conserved();
}

#[test]
fn deadline_boundaries_admit_at_deadline_and_reject_after_it() {
    let mut r = DataRun::new();
    let id = r.open(&["mri-copay"], N / 50);
    let d = r.job(id)["accept_deadline"].as_u64().unwrap();
    r.sim.advance_to(d - 2);
    let out = r.call(SELLER, "accept_job", json!({"job_id":id}), 0);
    out.assert_success();
    assert_eq!(out.receipt("accept_job").unwrap().block_height, d);
    let d = r.job(id)["work_deadline"].as_u64().unwrap();
    r.sim.advance_to(d - 2);
    let out = r.deliver(id, 0, "mri-copay");
    out.assert_success();
    assert_eq!(out.receipt("deliver").unwrap().block_height, d);
    let id = r.open(&["mri-copay"], N / 50);
    let d = r.job(id)["accept_deadline"].as_u64().unwrap();
    r.sim.advance_to(d);
    r.call(SELLER, "accept_job", json!({"job_id":id}), 0)
        .assert_panic_contains("acceptance expired");
    r.expire(id);
    let id = r.open(&["mri-copay"], N / 50);
    r.accept(id);
    let d = r.job(id)["work_deadline"].as_u64().unwrap();
    r.sim.advance_to(d);
    r.deliver(id, 0, "mri-copay")
        .assert_panic_contains("work expired");
    r.expire(id);
    r.assert_conserved();
}

#[test]
fn invalid_admission_and_proof_shapes_cannot_create_or_reprice_a_job() {
    let mut r = DataRun::new();
    for keys in [
        json!([]),
        json!(["mri-copay", "mri-copay"]),
        json!(["UPPERCASE"]),
        json!(vec!["x"; 9]),
    ] {
        let h = r.sim.height();
        let out=r.call(BUYER,"open_job",json!({"seller":SELLER,"dataset":"sample-v1","keys":keys,"price":"1","accept_deadline":h+50,"work_deadline":h+150}),ALLOWANCE);
        out.assert_failure();
        assert_eq!(r.view("get_job_count", json!({})), 0);
    }
    let id = r.open(&["mri-copay", "outpatient-mri"], 101);
    r.accept(id);
    let base = r.job(id);
    for variant in 0..5 {
        let mut opening = r.opening("mri-copay");
        match variant {
            0 => {
                opening["path"].as_array_mut().unwrap().pop();
            }
            1 => opening["salt"] = json!("not-a-hash"),
            2 => opening["path"][0] = json!("AB".repeat(32)),
            3 => opening["entry"]["source"] = json!("x".repeat(1025)),
            _ => opening["entry"] = json!(null),
        }
        r.call(
            SELLER,
            "deliver",
            json!({"job_id":id,"index":0,"opening":opening}),
            ALLOWANCE,
        )
        .assert_failure();
        assert_eq!(r.job(id), base);
    }
    r.deliver(id, 0, "mri-copay").assert_success();
    let paid = r.job(id);
    r.deliver(id, 0, "mri-copay")
        .assert_panic_contains("fact already resolved");
    assert_eq!(r.job(id), paid);
    r.call(SELLER, "close_job", json!({"job_id":id}), 0)
        .assert_success();
    assert_eq!(r.job(id)["paid"], "51");
    assert_eq!(r.job(id)["refunded"], "50");
    r.assert_conserved();
}

#[test]
fn independent_data_client_commands_complete_the_actual_contract_lifecycle() {
    use clap::Parser;
    use wbts_cli::data::{DataApp, DataCommand, Request};
    fn execute(r: &mut DataRun, command: DataCommand) -> serde_json::Value {
        match command.request(ALLOWANCE).unwrap() {
            Request::Send {
                signer,
                method,
                args,
                principal,
                allowance,
            } => {
                let out = r.call(&signer, method, args, principal + allowance);
                out.assert_success();
                out.json()
            }
            Request::Read { method, args } => r.view(method, args),
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("wbts-data-client-{}", std::process::id()));
    let script = root.join("scripts/data-facts.mjs");
    let input = root.join("data-escrow/compat/coverage.json");
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg("seal")
        .arg(&input)
        .arg(&dir)
        .output()
        .expect("Node 22.18+ is required for data tooling");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("catalog.json")).unwrap()).unwrap();
    let second = std::process::Command::new("node")
        .arg(&script)
        .arg("seal")
        .arg(&input)
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        !second.status.success(),
        "sealing must preserve an existing commitment"
    );
    let path = dir.join("opening.json");
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg("prove")
        .arg(dir.join("sealed.json"))
        .arg("mri-copay")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut r = DataRun::empty();
    execute(
        &mut r,
        DataCommand::Publish {
            seller: SELLER.into(),
            name: "sample-v1".into(),
            root: catalog["root"].as_str().unwrap().into(),
            source_uri: "".into(),
        },
    );
    let h = r.sim.height();
    let job = execute(
        &mut r,
        DataCommand::Open {
            buyer: BUYER.into(),
            seller: SELLER.into(),
            dataset: "sample-v1".into(),
            keys: vec!["mri-copay".into()],
            price: N / 50,
            accept_deadline: h + 50,
            work_deadline: h + 150,
        },
    );
    let id = job["id"].as_u64().unwrap();
    execute(
        &mut r,
        DataCommand::Accept {
            seller: SELLER.into(),
            job_id: id,
        },
    );
    execute(
        &mut r,
        DataCommand::Deliver {
            seller: SELLER.into(),
            job_id: id,
            index: 0,
            proof: path.clone(),
        },
    );
    std::fs::remove_dir_all(&dir).unwrap();
    for signer in [BUYER, SELLER] {
        let app = DataApp::parse_from([
            "data-cli",
            "--contract",
            CONTRACT,
            "withdraw",
            "--signer",
            signer,
        ]);
        assert_eq!(execute(&mut r, app.command), true);
    }
    assert_eq!(execute(&mut r, DataCommand::Accounting)["conserved"], true);
    r.assert_wallets(N / 50);
}
