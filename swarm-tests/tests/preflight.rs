use near_contract_sim::{Balance, Sim};

/// Exercise the simulator runtime before adding project contract scenarios.
#[test]
fn simulator_transfers_near() {
    let mut sim = Sim::new();
    sim.create_account("sender.near", Balance::from_near(10));
    sim.create_account("receiver.near", Balance::from_near(10));
    let before = sim.balance("receiver.near").as_yoctonear();
    let amount = Balance::from_near(1);

    let outcome = sim
        .tx("sender.near", "receiver.near")
        .transfer(amount)
        .transact();
    outcome.assert_success();
    assert_eq!(
        sim.balance("receiver.near").as_yoctonear(),
        before + amount.as_yoctonear()
    );
    let _effects = outcome.effects();
    sim.findings().assert_none();
}
