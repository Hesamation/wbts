mod common;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use common::*;
use near_contract_sim::{Balance, Fault, Gas, PromiseResult, Receipts};
use serde_json::{Value, json};

#[test]
fn own_collateral_two_edges_and_independent_release() {
    let mut r = Run::new();
    let registration = r.view("get_run", json!({"run_id":"r"}));
    assert_eq!(registration["agents"], json!([COORD, WORKER, WATCHER]));
    let a = r.offer(INPUT);
    assert_eq!(number(&r.edge(a)["worker_reserved"]), 0);
    r.call(
        COORD,
        "accept_assignment",
        json!({"edge_id":r.id(a)}),
        N + N / 20,
    )
    .assert_panic_contains("only worker");
    r.accept(a);
    r.complete(a, OUTPUT);
    let b = r.offer(INPUT);
    r.accept(b);
    r.wait_release(a);
    assert_eq!(r.edge(a)["released"], true);
    assert_eq!(number(&r.edge(b)["worker_reserved"]), N);
    let before = r.credit(WORKER);
    r.release(a);
    assert_eq!(r.credit(WORKER), before);
    assert_eq!(r.view("verify_chain", json!({"run_id":"r"})), Value::Null);
}
#[test]
fn abandoned_offer_returns_only_delegators_stake() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    let before = r.credit(COORD);
    let e = r.edge(a);
    r.call(OUTSIDER, "release_edge", json!({"edge_id":r.id(a)}), 0)
        .assert_panic_contains("offer still open");
    r.sim.advance_to(e["accept_deadline"].as_u64().unwrap() + 1);
    r.call(
        WORKER,
        "accept_assignment",
        json!({"edge_id":r.id(a)}),
        N + N / 20,
    )
    .assert_panic_contains("acceptance deadline");
    r.release(a);
    assert_eq!(r.credit(COORD), before + N);
    assert!(r.view("get_credit", json!({"account_id":WORKER})).is_null());
}
#[test]
fn unfinished_edges_release_without_timeout_blame() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    r.call(WATCHER, "file_claim", r.args(a, "F1"), N / 4 + N / 20)
        .assert_panic_contains("F1 requires completion");
    let reps = [r.rep(COORD), r.rep(WORKER)];
    r.wait_release(a);
    assert_eq!([r.rep(COORD), r.rep(WORKER)], reps);
    assert_eq!(r.view("get_treasury", json!({})), "0");
}
#[test]
fn exact_settlement_allocations_and_one_accepted_blame() {
    for challenged in [false, true] {
        let mut r = Run::new();
        let a = r.offer(INPUT);
        r.accept(a);
        r.complete(a, b"wrong");
        let id = r.file(a, "F1", COORD);
        if challenged {
            let o = r.challenge(id, WATCHER);
            assert_eq!(o.json::<Value>()["verdict"], "Upheld");
        }
        let c = r.credit(COORD);
        r.finalize(id);
        assert_eq!(
            r.credit(COORD) - c,
            if challenged {
                N * 65 / 100
            } else {
                N * 45 / 100
            }
        );
        assert_eq!(
            number(&r.view("get_treasury", json!({}))),
            if challenged { N / 10 } else { N / 20 }
        );
        assert_eq!(number(&r.edge(a)["worker_reserved"]), N * 3 / 4);
        assert_eq!(r.rep(WORKER), 90);
        for reporter in [COORD, WORKER, WATCHER, OUTSIDER] {
            r.call(reporter, "file_claim", r.args(a, "F1"), N / 4 + N / 20)
                .assert_panic_contains("edge not claimable");
        }
        let c = r.credit(COORD);
        r.call(OUTSIDER, "finalize", json!({"claim_id":id}), 0)
            .assert_success();
        assert_eq!(r.credit(COORD), c);
        r.release(a);
        assert_eq!(r.edge(a)["released"], true);
    }
}
#[test]
fn false_claim_stored_evidence_and_atomic_overturn() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    r.complete(a, OUTPUT);
    let id = r.file(a, "F1", COORD);
    let saved = r.claim(id)["payloads"].clone();
    r.bodies.clear();
    let out = r.challenge(id, WATCHER);
    assert_eq!(out.json::<Value>()["verdict"], "Overturned");
    assert_eq!(r.claim(id)["status"], "AdjudicatedOverturned");
    assert_eq!(r.claim(id)["payloads"], saved);
    let credit = r.credit(WATCHER);
    r.finalize(id);
    assert_eq!(r.credit(WATCHER) - credit, N * 45 / 100);
    assert_eq!(
        [r.rep(COORD), r.rep(WORKER), r.rep(WATCHER)],
        [85, 102, 105]
    );
    assert_eq!(number(&r.edge(a)["worker_reserved"]), N);
    r.call(WATCHER, "resolve_expired", json!({"claim_id":id}), 0)
        .assert_failure();
}
#[test]
fn corrected_claim_has_fresh_window_bond_and_no_duplicate_attempts() {
    let mut r = Run::new();
    let a = r.offer(BAD);
    r.accept(a);
    r.complete(a, b"wrong");
    let first = r.file(a, "F1", COORD);
    r.challenge(first, WORKER).assert_success();
    r.finalize(first);
    assert_eq!(r.rep(COORD), 85);
    assert_eq!(number(&r.edge(a)["delegator_reserved"]), N);
    let e = r.edge(a);
    assert!(
        e["corrected_deadline"].as_u64().unwrap() > e["completion_height"].as_u64().unwrap() + 60
    );
    r.call(WATCHER, "file_claim", r.args(a, "F1"), N / 4 + N / 20)
        .assert_panic_contains("allegation already attempted");
    r.call(OUTSIDER, "release_edge", json!({"edge_id":r.id(a)}), 0)
        .assert_panic_contains("claim window still open");
    let second = r.file(a, "F3", WORKER);
    r.challenge(second, COORD).assert_success();
    assert_eq!(
        r.claim(second)["challenge_deadline"].as_u64().unwrap()
            - r.claim(second)["claim_height"].as_u64().unwrap(),
        60
    );
    r.finalize(second);
    assert_eq!(number(&r.edge(a)["delegator_reserved"]), N * 3 / 4);
    assert_eq!(r.rep(COORD), 75);
    assert_eq!(r.rep(WORKER), 107);
    r.release(a);
}
#[test]
fn false_f3_before_completion_can_be_disproved_without_output() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    let id = r.file(a, "F3", WATCHER);
    let o = r.challenge(id, COORD);
    assert_eq!(o.json::<Value>()["verdict"], "Overturned");
    assert!(o.json::<Value>()["finding"].is_null());
    r.complete(a, b"wrong");
    r.finalize(id);
    assert_eq!(r.edge(a)["corrected_deadline"], 0);
    // Once work completion's normal window passes, there is no fabricated extension.
    let e = r.edge(a);
    r.sim
        .advance_to(e["completion_height"].as_u64().unwrap() + 61);
    r.call(WATCHER, "file_claim", r.args(a, "F1"), N / 4 + N / 20)
        .assert_panic_contains("claim window expired");
}
#[test]
fn evidence_authentication_underfunding_and_actor_checks_rollback() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.call(COORD, "file_claim", r.args(a, "F3"), N / 4 + N / 20)
        .assert_panic_contains("edge not claimable");
    r.accept(a);
    r.complete(a, OUTPUT);
    let args = r.args(a, "F1");
    let state = r.edge(a);
    let mut wrong = args.clone();
    wrong["payloads"][1]["bytes"] = json!(STANDARD.encode(b"wrong"));
    r.call(COORD, "file_claim", wrong, N / 4 + N / 20)
        .assert_panic_contains("payload hash mismatch");
    r.call(COORD, "file_claim", args.clone(), N / 4)
        .assert_panic_contains("insufficient principal");
    assert_eq!(r.edge(a), state);
    let mut duplicate = args.clone();
    duplicate["payloads"][1]["seq"] = json!(0);
    r.call(COORD, "file_claim", duplicate, N / 4 + N / 20)
        .assert_panic_contains("evidence sequence mismatch");
    let mut unsupported = args;
    unsupported["failure_class"] = json!("F2");
    r.call(COORD, "file_claim", unsupported, N / 4 + N / 20)
        .assert_panic_contains("unsupported claim");
    let id = r.file(a, "F1", COORD);
    let state = r.claim(id);
    r.call(WATCHER, "challenge", json!({"claim_id":id}), N / 4)
        .assert_panic_contains("insufficient principal");
    assert_eq!(r.claim(id), state);
    r.call(OUTSIDER, "file_claim", r.args(a, "F3"), N / 4 + N / 20)
        .assert_panic_contains("active claim");
}
#[test]
fn parameter_changes_do_not_reprice_existing_edges_and_slash_is_capped() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    r.complete(a, b"wrong");
    r.call(COORD,"update_parameters",json!({"stake":N.to_string(),"blame_bond":(2*N).to_string(),"challenge_bond":(2*N).to_string()}),0).assert_success();
    let id = r.file(a, "F1", COORD);
    assert_eq!(number(&r.claim(id)["bond"]), N / 4);
    r.finalize(id);
    r.release(a);
    let b = r.offer(INPUT);
    r.accept(b);
    r.complete(b, b"wrong");
    let out = r.call(WATCHER, "file_claim", r.args(b, "F1"), 2 * N + N / 20);
    out.assert_success();
    let id = out.json();
    let credit = r.credit(WATCHER);
    r.finalize(id);
    assert_eq!(r.credit(WATCHER) - credit, 2 * N + N * 4 / 5);
    assert_eq!(number(&r.edge(b)["worker_reserved"]), 0);
}
#[test]
fn unchallenged_false_claim_is_explicitly_optimistic_finality() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    r.complete(a, OUTPUT);
    let id = r.file(a, "F1", COORD);
    r.finalize(id);
    assert_eq!(r.claim(id)["economic_outcome"], "UnchallengedAccepted");
    assert!(r.claim(id)["adjudication_result"].is_null());
    assert_eq!(r.rep(WORKER), 90);
    r.call(WORKER, "challenge", json!({"claim_id":id}), N / 4 + N / 20)
        .assert_panic_contains("claim not pending");
}
#[test]
fn self_edge_requires_two_independently_reserved_stakes_and_roles_compose() {
    let mut r = Run::new();
    let a = r.offer_worker(INPUT, COORD);
    r.accept(a);
    assert_eq!(
        number(&r.edge(a)["delegator_reserved"]) + number(&r.edge(a)["worker_reserved"]),
        2 * N
    );
    r.complete(a, OUTPUT);
    let id = r.file(a, "F1", COORD);
    r.challenge(id, COORD).assert_success();
    r.finalize(id);
    assert_eq!(r.rep(COORD), 92);
    r.wait_release(a);
}
#[test]
fn successful_withdrawal_replays_and_private_callback() {
    let mut r = Run::new();
    let credit = r.credit(COORD);
    let balance = r.sim.balance(COORD).as_yoctonear();
    let o = r.call(COORD, "withdraw", json!({"amount":credit.to_string()}), 1);
    o.assert_success();
    assert!(o.json::<bool>());
    assert_eq!(r.credit(COORD), 1);
    assert_eq!(
        r.sim.balance(COORD).as_yoctonear(),
        balance - 1 + credit - o.effects().gas_paid().as_yoctonear()
    );
    let account = r.view("get_credit", json!({"account_id":COORD}));
    assert_eq!(account["in_flight"], "0");
    assert!(number(&account["retained_storage_paid"]) > 0);
    r.call(
        WATCHER,
        "on_withdraw",
        json!({"account_id":COORD,"nonce":1,"amount":credit.to_string()}),
        0,
    )
    .assert_failure();
    let o = r
        .sim
        .call(COORD, CONTRACT, "on_withdraw")
        .args_json(json!({"account_id":COORD,"nonce":1,"amount":credit.to_string()}))
        .promise_results([PromiseResult::Successful(vec![])])
        .transact();
    o.assert_success();
    assert!(!o.json::<bool>());
    assert_eq!(r.credit(COORD), 1);
    r.call(COORD, "withdraw", json!({"amount":"2"}), 0)
        .assert_panic_contains("exactly 1");
    r.call(
        COORD,
        "withdraw",
        json!({"amount":"999999999999999999999999999999"}),
        1,
    )
    .assert_panic_contains("invalid withdrawal");
}
#[test]
fn test_only_delayed_transfer_to_deleted_account_restores_credit_then_retry() {
    use near_contract_sim::nearcore::near_primitives::action::DeleteAccountAction;
    let mut r = Run::new();
    let amount = r.credit(COORD);
    let tx = r
        .sim
        .call(COORD, CONTRACT, "withdraw")
        .args_json(json!({"amount":amount.to_string()}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::action("Transfer"), Fault::Hold(20))
        .submit();
    r.sim.step();
    r.sim.step();
    assert_eq!(
        number(&r.view("get_credit", json!({"account_id":COORD}))["in_flight"]),
        amount
    );
    // Real runtime transfer failure; test-only scheduling creates the missing recipient.
    r.sim
        .tx(COORD, COORD)
        .action(DeleteAccountAction {
            beneficiary_id: WATCHER.parse().unwrap(),
        })
        .transact()
        .assert_success();
    r.sim.settle();
    let o = r.sim.outcome(tx);
    assert!(!o.json::<bool>());
    assert_eq!(r.credit(COORD), amount + 1);
    assert_eq!(
        r.view("get_credit", json!({"account_id":COORD}))["in_flight"],
        "0"
    );
    r.sim.create_account(COORD, Balance::from_near(10));
    let o = r.call(
        COORD,
        "withdraw",
        json!({"amount":(amount+1).to_string()}),
        1,
    );
    o.assert_success();
    assert!(o.json::<bool>());
}
#[test]
fn callback_failure_keeps_ambiguous_transfer_pending() {
    let mut r = Run::new();
    let amount = r.credit(COORD);
    let o = r
        .sim
        .call(COORD, CONTRACT, "withdraw")
        .args_json(json!({"amount":amount.to_string()}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::method("on_withdraw"), Fault::OutOfGas)
        .transact();
    assert!(o.receipts.iter().any(|x| x.failure().is_some()));
    assert_eq!(
        number(&r.view("get_credit", json!({"account_id":COORD}))["in_flight"]),
        amount
    );
    r.call(COORD, "withdraw", json!({"amount":"1"}), 1)
        .assert_panic_contains("already pending");
}
#[test]
fn independent_integer_vectors_still_match_causal_verifier() {
    let vectors: Value =
        serde_json::from_str(include_str!("../fixtures/t2_integer_vectors.json")).unwrap();
    for v in vectors.as_array().unwrap() {
        let mut r = Run::with_bps(v["fee_basis_points"].as_u64().unwrap() as u16);
        let total = v["total"].as_str().unwrap();
        let body = format!(r#"{{"entries":["{total}"],"declared_total":"{total}"}}"#);
        let a = r.offer(body.as_bytes());
        r.accept(a);
        r.complete(a, v["output"].as_str().unwrap().as_bytes());
        let id = r.file(a, "F1", WATCHER);
        assert_eq!(
            r.view("preview_adjudication", json!({"claim_id":id}))["verdict"],
            "Overturned"
        );
    }
}
#[test]
fn worst_case_payloads_fit_atomic_challenge_and_limits_reject_oversize() {
    let mut r = Run::new();
    let input = vec![b'x'; 8192];
    let output = vec![b'y'; 8192];
    let a = r.offer(&input);
    r.accept(a);
    r.complete(a, &output);
    let o = r.call(WATCHER, "file_claim", r.args(a, "F1"), N / 2);
    o.assert_success();
    println!(
        "max payload filing storage_bytes={}",
        o.effects().storage_bytes(CONTRACT)
    );
    let id = o.json();
    let out = r.challenge(id, WORKER);
    println!(
        "max payload atomic challenge gas={} storage_bytes={}",
        out.gas_burnt().as_gas(),
        out.effects().storage_bytes(CONTRACT)
    );
    assert!(out.gas_burnt().as_gas() < 100_000_000_000_000);
    assert_eq!(out.json::<Value>()["finding"]["failure_class"], "F3");
    let b = r.offer(&vec![b'z'; 8193]);
    r.accept(b);
    r.call(WATCHER, "file_claim", r.args(b, "F3"), N / 4 + N / 2)
        .assert_panic_contains("payload exceeds byte limit");
}

#[test]
fn full_event_prefix_does_not_block_atomic_challenge() {
    let mut r = Run::new();
    let a = r.offer(&vec![b'x'; 8192]);
    r.accept(a);
    for seq in 1..99 {
        let head = r.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
        let out=r.call(COORD,"anchor",json!({"run_id":"r","seq":seq,"actor":COORD,"kind":"note","payload_hash":digest(b"{}"),"prev_hash":head,"task":null}),N/20);
        out.assert_success();
        r.bodies.push(b"{}".to_vec());
        r.txs.push(out.hash.to_string());
    }
    r.complete(a, &vec![b'y'; 8192]);
    let o = r.call(WATCHER, "file_claim", r.args(a, "F1"), N / 2);
    o.assert_success();
    let id = o.json();
    let o = r.challenge(id, WORKER);
    println!(
        "100-event/16-KiB evidence challenge gas={}",
        o.gas_burnt().as_gas()
    );
    assert!(o.gas_burnt().as_gas() < 100_000_000_000_000);
    assert_eq!(r.claim(id)["evidence_snapshot"]["event_count"], 100);
}

#[test]
fn exact_acceptance_completion_filing_challenge_and_finalize_boundaries() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    let deadline = r.edge(a)["accept_deadline"].as_u64().unwrap();
    r.sim.advance_to(deadline - 2);
    let o = r.call(
        WORKER,
        "accept_assignment",
        json!({"edge_id":r.id(a)}),
        N + N / 20,
    );
    o.assert_success();
    assert_eq!(
        o.receipt("accept_assignment").unwrap().block_height,
        deadline
    );
    let d = r.edge(a)["work_deadline"].as_u64().unwrap();
    r.sim.advance_to(d - 2);
    r.complete(a, OUTPUT);
    assert_eq!(r.edge(a)["completion_height"], d);
    let cutoff = r.edge(a)["normal_claim_deadline"].as_u64().unwrap();
    r.sim.advance_to(cutoff - 2);
    let id = r.file(a, "F1", WATCHER);
    assert_eq!(r.claim(id)["claim_height"], cutoff);
    let cd = r.claim(id)["challenge_deadline"].as_u64().unwrap();
    r.sim.advance_to(cd - 2);
    let o = r.challenge(id, COORD);
    assert_eq!(o.receipt("challenge").unwrap().block_height, cd);
    r.finalize(id);
    assert_eq!(r.claim(id)["status"], "Finalized");
}
#[test]
fn after_deadline_calls_reject_and_early_finalize_never_slashes() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    let d = r.edge(a)["work_deadline"].as_u64().unwrap();
    r.sim.advance_to(d);
    let head = r.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
    r.call(WORKER,"anchor",json!({"run_id":"r","seq":1,"actor":WORKER,"kind":"settlement","payload_hash":digest(OUTPUT),"prev_hash":head,"task":{"assignment_seq":a,"worker":WORKER,"parent_assignment_seq":null}}),N/20).assert_panic_contains("work deadline expired");
    let id = r.file(a, "F3", WATCHER);
    r.call(COORD, "finalize", json!({"claim_id":id}), 0)
        .assert_panic_contains("challenge window still open");
    assert_eq!(number(&r.edge(a)["delegator_reserved"]), N);
    let deadline = r.claim(id)["challenge_deadline"].as_u64().unwrap();
    r.sim.advance_to(deadline);
    r.call(COORD, "challenge", json!({"claim_id":id}), N / 4 + N / 20)
        .assert_panic_contains("challenge window expired");
}
#[test]
fn economic_conservation_includes_credits_storage_reservations_and_withdrawals() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    r.accept(a);
    r.complete(a, b"wrong");
    let id = r.file(a, "F1", WATCHER);
    r.challenge(id, WORKER).assert_success();
    r.finalize(id);
    r.release(a);
    // Registration + offer + accept + complete + file + challenge deposits.
    let deposited =
        N / 20 + (N + N / 20) + (N + N / 20) + N / 20 + (N / 4 + N / 20) + (N / 4 + N / 20);
    let mut allocated = number(&r.view("get_treasury", json!({})));
    for who in [COORD, WORKER, WATCHER] {
        let c = r.view("get_credit", json!({"account_id":who}));
        allocated +=
            number(&c["available"]) + number(&c["in_flight"]) + number(&c["retained_storage_paid"]);
    }
    assert_eq!(allocated, deposited);
    assert_eq!(
        number(&r.claim(id)["bond"]) + number(&r.claim(id)["challenge_bond"]),
        0
    );
}
#[test]
fn pending_withdrawal_rejects_second_but_accepts_new_credit() {
    let mut r = Run::new();
    let amount = r.credit(COORD);
    r.sim
        .call(COORD, CONTRACT, "withdraw")
        .args_json(json!({"amount":amount.to_string()}))
        .deposit(Balance::from_yoctonear(1))
        .gas(Gas::from_teragas(100))
        .inject(Receipts::method("on_withdraw"), Fault::Hold(30))
        .submit();
    r.sim.step();
    r.sim.step();
    let rejected = r
        .sim
        .call(COORD, CONTRACT, "withdraw")
        .args_json(json!({"amount":"1"}))
        .deposit(Balance::from_yoctonear(1))
        .submit();
    r.sim.step();
    r.sim.step();
    assert!(r.sim.outcome(rejected).failure().is_some());
    let a = r.offer(INPUT);
    r.sim.settle();
    assert_eq!(
        r.view("get_credit", json!({"account_id":COORD}))["in_flight"],
        "0"
    );
    assert!(r.credit(COORD) > 1);
    assert_eq!(number(&r.edge(a)["delegator_reserved"]), N);
}

#[test]
fn reputation_saturates_and_success_bonus_is_once_per_run() {
    let mut r = Run::new();
    for i in 0..14 {
        let a = r.offer(INPUT);
        r.accept(a);
        r.complete(a, OUTPUT);
        let id = r.file(a, "F1", COORD);
        r.challenge(id, WORKER).assert_success();
        r.finalize(id);
        r.wait_release(a);
        assert_eq!(r.rep(COORD), 100u64.saturating_sub(15 * (i + 1)));
        assert_eq!(r.rep(WORKER), (105 + 2 * (i + 1)).min(120));
    }
}
#[test]
fn assignment_and_completion_cannot_bypass_collateral_or_actor_binding() {
    let mut r = Run::new();
    let a = r.offer(INPUT);
    let head = r.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
    let completion = json!({"run_id":"r","seq":1,"actor":WORKER,"kind":"settlement","payload_hash":digest(OUTPUT),"prev_hash":head,"task":{"assignment_seq":a,"worker":WORKER,"parent_assignment_seq":null}});
    r.call(WORKER, "anchor", completion.clone(), N / 20)
        .assert_panic_contains("edge not active");
    r.accept(a);
    r.call(WATCHER, "anchor", completion.clone(), N / 20)
        .assert_panic_contains("actor must be caller");
    let mut spoof = completion.clone();
    spoof["actor"] = json!(WATCHER);
    r.call(WATCHER, "anchor", spoof, N / 20)
        .assert_panic_contains("only worker");
    let mut nested = completion;
    nested["task"]["parent_assignment_seq"] = json!(0);
    r.call(WORKER, "anchor", nested, N / 20)
        .assert_panic_contains("nested delegation");
    r.call(COORD,"anchor",json!({"run_id":"r","seq":1,"actor":COORD,"kind":"task_assigned","payload_hash":digest(INPUT),"prev_hash":head,"task":null}),N/20).assert_panic_contains("use offer_assignment");
    r.complete(a, OUTPUT);
    let head = r.view("get_run", json!({"run_id":"r"}))["head_hash"].clone();
    r.call(COORD,"offer_assignment",json!({"run_id":"r","seq":2,"worker":WORKER,"payload_hash":digest(INPUT),"prev_hash":head,"accept_deadline":u64::MAX-1,"work_deadline":u64::MAX}),N+N/20).assert_panic_contains("height overflow");
}
