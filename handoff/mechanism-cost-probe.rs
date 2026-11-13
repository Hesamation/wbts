mod common;
use common::*;
use serde_json::{json, Value};

#[test]
fn research_verification_cost() {
    let many = serde_json::to_vec(&json!({"entries":vec!["1";128],"declared_total":"128"})).unwrap();
    let padded = { let mut b=many.clone(); b.resize(8192,b' '); b };
    for (name,input,output,class) in [
        ("valid-small",INPUT.to_vec(),OUTPUT.to_vec(),"F1"),
        ("bad-output",INPUT.to_vec(),b"wrong".to_vec(),"F1"),
        ("invalid-input",BAD.to_vec(),OUTPUT.to_vec(),"F1"),
        ("valid-128-padded",padded,br#"{"fee":"3","net":"125"}"#.to_vec(),"F1"),
    ] {
        let mut r=Run::new();
        let a=r.offer(&input); r.accept(a); r.complete(a,&output);
        let file=r.call(WATCHER,"file_claim",r.args(a,class),N);
        file.assert_success(); let id:u64=file.json();
        let before=r.claim(id);
        let check=r.call(WORKER,"preview_adjudication",json!({"claim_id":id}),0);
        check.assert_success();
        assert_eq!(r.claim(id),before);
        let verdict:Value=check.json();
        let expected=if name=="bad-output" {"Upheld"} else {"Overturned"};
        assert_eq!(verdict["verdict"],expected);
        let challenge=r.challenge(id,WORKER);
        let h=r.claim(id)["challenge_deadline"].as_u64().unwrap();
        r.sim.advance_to(h+1);
        let finalize=r.call(OUTSIDER,"finalize",json!({"claim_id":id}),0);
        finalize.assert_success();
        println!("RESEARCH {} input={} output={} file_gas={} file_storage={} check_tx_gas={} challenge_gas={} finalize_gas={} verdict={}",name,input.len(),output.len(),file.gas_burnt().as_gas(),file.effects().storage_bytes(CONTRACT),check.gas_burnt().as_gas(),challenge.gas_burnt().as_gas(),finalize.gas_burnt().as_gas(),verdict);
    }
}
