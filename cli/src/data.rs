//! Independent participant client for the proof-backed data escrow.
use crate::{NEAR, Transport, account, hash, live::Live};
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Parser, Debug)]
#[command(about = "Proof-backed data jobs on NEAR testnet; no account creation or deployment")]
pub struct DataApp {
    /// Explicit fresh data-escrow contract; there is no legacy-contract default.
    #[arg(long)]
    pub contract: String,
    #[arg(long, global = true, default_value = "https://rpc.testnet.near.org")]
    pub rpc_url: String,
    #[arg(long, global = true, default_value = ".wbts-data")]
    pub journal_dir: PathBuf,
    /// Before signing, deployed code must match these exact bytes.
    #[arg(
        long,
        global = true,
        default_value = "data-escrow/target/near/wbts_data_escrow.wasm"
    )]
    pub wasm: PathBuf,
    #[arg(long, global = true, default_value = "50000000000000000000000")]
    pub storage_allowance: u128,
    #[command(subcommand)]
    pub command: DataCommand,
}

#[derive(Subcommand, Debug)]
pub enum DataCommand {
    Publish {
        #[arg(long)]
        seller: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        root: String,
        #[arg(long, default_value = "")]
        source_uri: String,
    },
    Open {
        #[arg(long)]
        buyer: String,
        #[arg(long)]
        seller: String,
        #[arg(long)]
        dataset: String,
        #[arg(long,num_args=1..=8)]
        keys: Vec<String>,
        #[arg(long)]
        price: u128,
        #[arg(long)]
        accept_deadline: u64,
        #[arg(long)]
        work_deadline: u64,
    },
    Accept {
        #[arg(long)]
        seller: String,
        #[arg(long)]
        job_id: u64,
    },
    Deliver {
        #[arg(long)]
        seller: String,
        #[arg(long)]
        job_id: u64,
        #[arg(long)]
        index: u8,
        /// JSON opening containing entry, salt and 32 path siblings (or a vacancy).
        #[arg(long)]
        proof: PathBuf,
    },
    Cancel {
        #[arg(long)]
        buyer: String,
        #[arg(long)]
        job_id: u64,
    },
    Close {
        #[arg(long)]
        seller: String,
        #[arg(long)]
        job_id: u64,
    },
    Expire {
        #[arg(long)]
        signer: String,
        #[arg(long)]
        job_id: u64,
    },
    /// Omit amount to withdraw all credit, including the attached security yocto.
    Withdraw {
        #[arg(long)]
        signer: String,
        #[arg(long)]
        amount: Option<u128>,
    },
    Job {
        #[arg(long)]
        job_id: u64,
    },
    Dataset {
        #[arg(long)]
        seller: String,
        #[arg(long)]
        name: String,
    },
    Delivery {
        #[arg(long)]
        job_id: u64,
        #[arg(long)]
        index: u8,
    },
    Balance {
        #[arg(long)]
        account: String,
    },
    Accounting,
    Protocol,
}

pub enum Request {
    Read {
        method: &'static str,
        args: Value,
    },
    Send {
        signer: String,
        method: &'static str,
        args: Value,
        principal: u128,
        allowance: u128,
    },
}

impl DataCommand {
    pub fn request(self, storage: u128) -> Result<Request> {
        let send = |signer: String, method, args, principal, allowance| -> Result<Request> {
            account(&signer)?;
            Ok(Request::Send {
                signer,
                method,
                args,
                principal,
                allowance,
            })
        };
        Ok(match self {
            Self::Publish {
                seller,
                name,
                root,
                source_uri,
            } => {
                return send(
                    seller,
                    "publish_dataset",
                    json!({"name":name,"root":root,"source_uri":source_uri}),
                    0,
                    storage,
                );
            }
            Self::Open {
                buyer,
                seller,
                dataset,
                keys,
                price,
                accept_deadline,
                work_deadline,
            } => {
                account(&seller)?;
                return send(
                    buyer,
                    "open_job",
                    json!({"seller":seller,"dataset":dataset,"keys":keys,"price":price.to_string(),"accept_deadline":accept_deadline,"work_deadline":work_deadline}),
                    price,
                    storage,
                );
            }
            Self::Accept { seller, job_id } => {
                return send(seller, "accept_job", json!({"job_id":job_id}), 0, 0);
            }
            Self::Deliver {
                seller,
                job_id,
                index,
                proof,
            } => {
                let bytes = fs::read(proof).context("read opening file")?;
                ensure!(bytes.len() <= 8192, "opening file exceeds 8 KiB");
                let opening: Value =
                    serde_json::from_slice(&bytes).context("opening must be JSON")?;
                return send(
                    seller,
                    "deliver",
                    json!({"job_id":job_id,"index":index,"opening":opening}),
                    0,
                    storage,
                );
            }
            Self::Cancel { buyer, job_id } => {
                return send(buyer, "cancel_job", json!({"job_id":job_id}), 0, 0);
            }
            Self::Close { seller, job_id } => {
                return send(seller, "close_job", json!({"job_id":job_id}), 0, 0);
            }
            Self::Expire { signer, job_id } => {
                return send(signer, "expire_job", json!({"job_id":job_id}), 0, 0);
            }
            Self::Withdraw { signer, amount } => {
                return send(
                    signer,
                    "withdraw",
                    json!({"amount":amount.map(|n|n.to_string())}),
                    1,
                    0,
                );
            }
            Self::Job { job_id } => Request::Read {
                method: "get_job",
                args: json!({"job_id":job_id}),
            },
            Self::Dataset { seller, name } => Request::Read {
                method: "get_dataset",
                args: json!({"seller":seller,"name":name}),
            },
            Self::Delivery { job_id, index } => Request::Read {
                method: "get_delivery",
                args: json!({"job_id":job_id,"index":index}),
            },
            Self::Balance { account } => Request::Read {
                method: "get_credit",
                args: json!({"account_id":account}),
            },
            Self::Accounting => Request::Read {
                method: "get_accounting",
                args: json!({}),
            },
            Self::Protocol => Request::Read {
                method: "get_protocol",
                args: json!({}),
            },
        })
    }
}

pub fn run(app: DataApp) -> Result<()> {
    account(&app.contract)?;
    ensure!(
        app.storage_allowance <= NEAR / 4,
        "storage allowance exceeds 0.25 NEAR"
    );
    let request = app.command.request(app.storage_allowance)?;
    // Read artifact before any possible signing; read-only commands do not need it.
    let expected = match &request {
        Request::Send { .. } => Some(hash(
            &fs::read(&app.wasm).context("build data-escrow WASM before sending")?,
        )),
        _ => None,
    };
    let mut live = Live::connect_testnet(
        app.rpc_url,
        app.contract,
        app.journal_dir.join("transactions"),
    )?;
    let protocol = live.view("get_protocol", json!({}))?;
    ensure!(
        protocol["name"] == "wbts-data-escrow" && protocol["version"] == "1.0.0",
        "incompatible data-escrow ABI"
    );
    match request {
        Request::Read { method, args } => println!(
            "{}",
            serde_json::to_string_pretty(&live.view(method, args)?)?
        ),
        Request::Send {
            signer,
            method,
            args,
            principal,
            allowance,
        } => {
            live.require_code_hash(expected.as_ref().unwrap())?;
            let tx = live.call_data(&signer, method, args, principal, allowance)?;
            if method == "withdraw" {
                ensure!(
                    tx.result == true,
                    "transfer failed; credit restored, inspect before retrying"
                );
            }
            let accounting = live.view("get_accounting", json!({}))?;
            ensure!(
                accounting["conserved"] == true,
                "accounting inconsistency; stop and inspect receipts"
            );
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"transaction":tx.hash,"result":tx.result,"accounting":accounting})
                )?
            );
        }
    }
    Ok(())
}
