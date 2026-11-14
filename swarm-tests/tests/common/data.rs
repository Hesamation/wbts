#![allow(dead_code)]
use near_contract_sim::{Balance, Gas, Sim, TxOutcome};
use serde_json::{Value, json};
use std::sync::OnceLock;

pub const CONTRACT: &str = "data-escrow.testnet";
pub const BUYER: &str = "buyer.testnet";
pub const SELLER: &str = "specialist.testnet";
pub const OTHER: &str = "observer.testnet";
pub const N: u128 = 1_000_000_000_000_000_000_000_000;
pub const ALLOWANCE: u128 = N / 20;

pub fn wasm() -> &'static Vec<u8> {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data-escrow/Cargo.toml")
            .canonicalize()
            .unwrap();
        let path = cargo_near_build::build_with_cli(
            cargo_near_build::BuildOpts::builder()
                .manifest_path(manifest.to_str().unwrap())
                .build(),
        )
        .expect("build current data escrow source");
        std::fs::read(path).unwrap()
    })
}

pub fn num(value: &Value) -> u128 {
    value.as_str().expect("integer string").parse().unwrap()
}

pub struct DataRun {
    pub sim: Sim,
    pub fixture: Value,
    pub steps: Vec<Value>,
    pub initial_wallets: Value,
}

impl DataRun {
    pub fn empty() -> Self {
        let mut sim = Sim::new();
        for account in [BUYER, SELLER, OTHER] {
            sim.create_account(account, Balance::from_near(100));
        }
        sim.deploy(CONTRACT, wasm().clone());
        sim.call(BUYER, CONTRACT, "new")
            .args_json(json!({}))
            .transact()
            .assert_success();
        let fixture =
            serde_json::from_slice(include_bytes!("../../../data-escrow/compat/fixtures.json"))
                .unwrap();
        let mut run = Self {
            sim,
            fixture,
            steps: vec![],
            initial_wallets: Value::Null,
        };
        run.initial_wallets = run.wallets();
        run
    }
    pub fn new() -> Self {
        let mut run = Self::empty();
        run.call(SELLER,"publish_dataset",json!({"name":"sample-v1","root":run.fixture["root"],"source_uri":"https://github.com/taejeekim/InferPay"}),ALLOWANCE).assert_success();
        run
    }
    pub fn view(&self, method: &str, args: Value) -> Value {
        self.sim.view(CONTRACT, method, args).json()
    }
    pub fn wallets(&self) -> Value {
        let mut rows = serde_json::Map::new();
        for account in [BUYER, SELLER, OTHER] {
            rows.insert(
                account.into(),
                json!(self.sim.balance(account).as_yoctonear().to_string()),
            );
        }
        Value::Object(rows)
    }
    pub fn credit(&self, account: &str) -> Value {
        self.view("get_credit", json!({"account_id":account}))
    }
    pub fn job(&self, id: u64) -> Value {
        self.view("get_job", json!({"job_id":id}))
    }
    pub fn accounting(&self) -> Value {
        self.view("get_accounting", json!({}))
    }
    pub fn call(&mut self, signer: &str, method: &str, args: Value, deposit: u128) -> TxOutcome {
        let out = self
            .sim
            .call(signer, CONTRACT, method)
            .args_json(args.clone())
            .deposit(Balance::from_yoctonear(deposit))
            .gas(Gas::from_teragas(100))
            .transact();
        let result = if out.is_success() && !out.bytes().is_empty() {
            out.json::<Value>()
        } else {
            Value::Null
        };
        let job_id = args["job_id"].as_u64().or(result["id"].as_u64());
        let job = job_id.map(|id| self.job(id)).unwrap_or(Value::Null);
        self.steps.push(json!({"signer":signer,"method":method,"args":args,"deposit_yocto":deposit.to_string(),
            "hash":out.hash.to_string(),"success":out.is_success(),"gas_burnt":out.gas_burnt().as_gas(),
            "block_height":out.receipt(method).map(|r|r.block_height),"result":result,"job":job,"failure":format!("{:?}",out.failure()),
            "gas_paid_yocto":out.effects().gas_paid().as_yoctonear().to_string(),
            "storage_bytes_delta":out.effects().storage_bytes(CONTRACT),"receipt":out,
            "job_count":self.view("get_job_count",json!({})),"accounting":self.accounting()}));
        out
    }
    pub fn open(&mut self, keys: &[&str], price: u128) -> u64 {
        let h = self.sim.height();
        self.call(BUYER,"open_job",json!({"seller":SELLER,"dataset":"sample-v1","keys":keys,"price":price.to_string(),"accept_deadline":h+50,"work_deadline":h+150}),price+ALLOWANCE).json::<Value>()
            .get("id").unwrap().as_u64().unwrap()
    }
    pub fn accept(&mut self, id: u64) {
        self.call(SELLER, "accept_job", json!({"job_id":id}), 0)
            .assert_success();
    }
    pub fn opening(&self, key: &str) -> Value {
        self.fixture["proofs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["key"] == key)
            .unwrap()["opening"]
            .clone()
    }
    pub fn deliver(&mut self, id: u64, index: u8, key: &str) -> TxOutcome {
        self.call(
            SELLER,
            "deliver",
            json!({"job_id":id,"index":index,"opening":self.opening(key)}),
            ALLOWANCE,
        )
    }
    pub fn expire(&mut self, id: u64) {
        let job = self.job(id);
        let field = if job["accepted"] == true {
            "work_deadline"
        } else {
            "accept_deadline"
        };
        let target = job[field].as_u64().unwrap() + 1;
        if self.sim.height() < target {
            self.sim.advance_to(target);
        }
        self.call(OTHER, "expire_job", json!({"job_id":id}), 0)
            .assert_success();
    }
    pub fn withdraw_all(&mut self, account: &str) -> TxOutcome {
        self.call(account, "withdraw", json!({"amount":null}), 1)
    }
    pub fn assert_conserved(&self) {
        let audit = self.accounting();
        assert_eq!(audit["conserved"], true);
        for step in &self.steps {
            assert_eq!(step["accounting"]["conserved"], true);
        }
    }
    pub fn assert_wallets(&self, seller_revenue: u128) -> Vec<Value> {
        let mut rows = vec![];
        for account in [BUYER, SELLER, OTHER] {
            let gas = self
                .steps
                .iter()
                .filter(|s| s["signer"] == account)
                .map(|s| num(&s["gas_paid_yocto"]))
                .sum::<u128>();
            let credit = self.credit(account);
            let storage = credit.get("retained_storage_paid").map(num).unwrap_or(0);
            let available = credit.get("available").map(num).unwrap_or(0);
            let inflight = credit.get("in_flight").map(num).unwrap_or(0);
            assert_eq!(inflight, 0);
            let wallet =
                num(&self.wallets()[account]) as i128 - num(&self.initial_wallets[account]) as i128;
            let economic = wallet + gas as i128 + storage as i128 + available as i128;
            let expected = if account == SELLER {
                seller_revenue as i128
            } else if account == BUYER {
                -(seller_revenue as i128)
            } else {
                0
            };
            assert_eq!(economic, expected, "wallet reconciliation for {account}");
            rows.push(json!({"account":account,"wallet_delta_yocto":wallet.to_string(),"gas_paid_yocto":gas.to_string(),"retained_storage_paid_yocto":storage.to_string(),"available_yocto":available.to_string(),"economic_delta_yocto":economic.to_string()}));
        }
        rows
    }
}
