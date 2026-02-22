//! THE AMM FULL TEST GOES HERE.
//! This is the full AMM flow test.
#![allow(warnings)]
mod common;
use common::litesvm_deposit_tests::deposit_liquidity;
use common::litesvm_withdraw_tests::withdraw_liquidity;
use common::litesvm_setup::setup_initialized_amm;
use common::litesvm_swap_tests::swap_tokens;

#[test]
fn test_full_amm() {
    let mut ctx = setup_initialized_amm();
    let deposit_state = deposit_liquidity(&mut ctx);
    swap_tokens(&mut ctx);
    withdraw_liquidity(&mut ctx, &deposit_state);
}
