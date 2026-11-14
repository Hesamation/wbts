//! Execute scripted economic agents against real contract WASM, then export receipts.
#[path = "../tests/common/data.rs"]
mod data;
use anyhow::Result;
use data::*;
use serde_json::json;
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo/evidence"));
    fs::create_dir_all(&output)?;
    let cases = [
        (
            "buyer-disappears",
            "Buyer won't approve",
            "Valid proofs release payment without a buyer approval transaction.",
        ),
        (
            "forged-answer",
            "Seller invents a value",
            "The proof fails. No job payment moves; expiry refunds the buyer.",
        ),
        (
            "partial-delivery",
            "One fact never arrives",
            "The specialist earns only the delivered fact's share. The remainder is refunded.",
        ),
        (
            "proven-absence",
            "The dataset has no answer",
            "An authenticated empty slot refunds that fact immediately.",
        ),
        (
            "seller-disappears",
            "Seller never accepts",
            "Anyone can close an expired offer and credit the full price back to the buyer.",
        ),
        (
            "buyer-cancels",
            "Buyer cancels an offer",
            "An unaccepted offer can be cancelled. Late acceptance cannot revive it.",
        ),
    ];
    let mut manifest = vec![];
    for (id, title, description) in cases {
        let mut r = DataRun::new();
        let keys: Vec<&str> = match id {
            "forged-answer" | "seller-disappears" | "buyer-cancels" => vec!["mri-copay"],
            "proven-absence" => vec!["mri-copay", "missing-fact"],
            _ => vec!["outpatient-mri", "mri-copay"],
        };
        let job = r.open(&keys, N / 50);
        let revenue = match id {
            "buyer-disappears" => {
                r.accept(job);
                r.deliver(job, 0, keys[0]).assert_success();
                r.deliver(job, 1, keys[1]).assert_success();
                N / 50
            }
            "forged-answer" => {
                r.accept(job);
                let mut opening = r.opening("mri-copay");
                opening["entry"]["value"] = json!("10");
                r.call(
                    SELLER,
                    "deliver",
                    json!({"job_id":job,"index":0,"opening":opening}),
                    ALLOWANCE,
                )
                .assert_panic_contains("proof does not open");
                r.expire(job);
                0
            }
            "partial-delivery" => {
                r.accept(job);
                r.deliver(job, 0, keys[0]).assert_success();
                r.expire(job);
                N / 100
            }
            "proven-absence" => {
                r.accept(job);
                r.deliver(job, 0, keys[0]).assert_success();
                r.deliver(job, 1, keys[1]).assert_success();
                N / 100
            }
            "seller-disappears" => {
                r.expire(job);
                0
            }
            "buyer-cancels" => {
                r.call(BUYER, "cancel_job", json!({"job_id":job}), 0)
                    .assert_success();
                r.call(SELLER, "accept_job", json!({"job_id":job}), 0)
                    .assert_panic_contains("job is closed");
                0
            }
            _ => unreachable!(),
        };
        for account in [BUYER, SELLER] {
            let out = r.withdraw_all(account);
            out.assert_success();
            assert!(out.json::<bool>());
            assert_eq!(r.credit(account)["available"], "0");
            assert_eq!(r.credit(account)["in_flight"], "0");
        }
        r.assert_conserved();
        let rows = r.assert_wallets(revenue);
        let accounting = r.accounting();
        assert_eq!(accounting["locked"], "0");
        assert_eq!(accounting["available"], "0");
        assert_eq!(accounting["in_flight"], "0");
        let deliveries = (0..keys.len())
            .map(|index| r.view("get_delivery", json!({"job_id":job,"index":index})))
            .collect::<Vec<_>>();
        let evidence = json!({"id":id,"title":title,"description":description,
            "environment":"local NEAR contract simulator; constructed public dataset; no testnet transactions",
            "wasm_sha256":wbts_cli::hash(wasm()),"dataset":r.view("get_dataset",json!({"seller":SELLER,"name":"sample-v1"})),
            "initial_wallets":r.initial_wallets,"final_wallets":r.wallets(),"wallet_reconciliation":rows,
            "job":r.job(job),"deliveries":deliveries,"steps":r.steps,"accounting":accounting,
            "checks":{"wallets_reconciled":true,"credits_withdrawn":true,"ledger_conserved":true},
            "fixture_provenance":r.fixture["provenance"]});
        let bytes = serde_json::to_vec_pretty(&evidence)?;
        let file = format!("{id}.json");
        fs::write(output.join(&file), &bytes)?;
        manifest.push(json!({"id":id,"title":title,"description":description,"file":file,"sha256":wbts_cli::hash(&bytes)}));
        println!("PASS {id}: verified settlement, wallet accounting and withdrawal callbacks");
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(
            &json!({"schema_version":1,"environment":"local NEAR contract simulator","wasm_sha256":wbts_cli::hash(wasm()),"cases":manifest}),
        )?,
    )?;
    println!(
        "Exported {} actual contract runs to {}",
        manifest.len(),
        output.display()
    );
    Ok(())
}
