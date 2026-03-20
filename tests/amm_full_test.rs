//! THE AMM FULL TEST GOES HERE.
//! This is the full AMM flow test.
#![allow(warnings)]
mod common;
use common::litesvm_deposit_tests::deposit_liquidity;
use common::litesvm_withdraw_tests::withdraw_liquidity;
use common::litesvm_setup::setup_initialized_amm;
use common::litesvm_swap_tests::{
    normal_swap, zero_amount_swap, slippage_protected_swap,
};

#[test]
fn test_full_amm() {
    // =================== NORMAL SWAP TEST ========================
    let mut ctx_1 = setup_initialized_amm();
    // Pool token amounts
    let x_1_amount = 1_000_000;
    let y_1_amount = 1_000_000;
    let _ = deposit_liquidity(&mut ctx_1, x_1_amount, y_1_amount);
    // Swap parameters.
    let swap_amount_1 = 10_000;
    let slippage_1 = 9_800;
    let swap_x_1 = 1; // We are swapping toke X for Y
    normal_swap(&mut ctx_1, swap_amount_1, slippage_1, swap_x_1);
    println!(" ");

    // =================== ZERO SWAP TEST =========================
    let mut ctx_2 = setup_initialized_amm();
    // Pool token amounts
    let x_2_amount = 1_000_000;
    let y_2_amount = 1_000_000;
    let _ = deposit_liquidity(&mut ctx_2, x_2_amount, y_2_amount);
    // Swap parameters.
    let swap_amount_2 = 0;
    let slippage_2 = 1;
    let swap_x_2 = 1; // We are swapping toke X for Y
    zero_amount_swap(&mut ctx_2, swap_amount_2, slippage_2, swap_x_2);
    println!(" ");

    // =================== SLIPPAGE PROTECTION SWAP TEST ==========
    let mut ctx_3 = setup_initialized_amm();
    // Pool token amounts
    let x_3_amount = 100_000_000;
    let y_3_amount = 1_000_000;
    let _ = deposit_liquidity(&mut ctx_3, x_3_amount, y_3_amount);
    // Swap parameters.
    let swap_amount_3 = 5_000;
    let slippage_3 = 500;
    let swap_x_3 = 1; // We are swapping toke X for Y
    slippage_protected_swap(&mut ctx_3, swap_amount_3, slippage_3, swap_x_3);
    println!(" ");

    //withdraw_liquidity(&mut ctx, &deposit_state);
}
