use clap::{CommandFactory, Parser};
use wbts_cli::data::DataApp;

#[test]
fn data_client_has_a_valid_command_tree_and_no_legacy_contract_default() {
    DataApp::command().debug_assert();
    assert!(DataApp::try_parse_from(["data-cli", "accounting"]).is_err());
    let app = DataApp::try_parse_from([
        "data-cli",
        "--contract",
        "data.testnet",
        "withdraw",
        "--signer",
        "buyer.testnet",
    ])
    .unwrap();
    let request = app.command.request(0).unwrap();
    match request {
        wbts_cli::data::Request::Send {
            method,
            args,
            principal,
            allowance,
            ..
        } => {
            assert_eq!(method, "withdraw");
            assert!(args["amount"].is_null());
            assert_eq!(principal, 1);
            assert_eq!(allowance, 0);
        }
        _ => panic!("withdraw is a transaction"),
    }
}
