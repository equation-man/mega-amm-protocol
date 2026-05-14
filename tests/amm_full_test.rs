//! THE AMM FULL TEST GOES HERE.
//! This is the full AMM flow test.
#![allow(warnings)]
use proptest::prelude::*;
mod common;
use common::litesvm_deposit_tests::deposit_liquidity;
use common::litesvm_withdraw_tests::withdraw_liquidity;
use common::litesvm_setup::setup_initialized_amm;
use common::litesvm_swap_tests::{
    normal_swap, zero_amount_swap, slippage_protected_swap,
};

#[test]
fn test_full_amm() {
    // =================== NORMAL SWAP TEST(Balanced pool) ========================
    // Near zero slippage expected with 1:1 exchange rate on a balanced stable pool
    //let mut ctx_1 = setup_initialized_amm();
    //// Pool token amounts
    //let x_1_amount = 1_000_000;
    //let y_1_amount = 1_000_000;
    //let _ = deposit_liquidity(&mut ctx_1, x_1_amount, y_1_amount);
    //// Swap parameters.
    //let swap_amount_1 = 900_000;
    //let slippage_1 = 9_800;
    //let swap_x_1 = 1;
    //normal_swap(&mut ctx_1, swap_amount_1, slippage_1, swap_x_1);
    //println!(" ");

    // =================== ZERO SWAP TEST =========================
    // Testing zero swap amount guard to reject zero swaps.
    //let mut ctx_2 = setup_initialized_amm();
    //// Pool token amounts
    //let x_2_amount = 1_000_000;
    //let y_2_amount = 1_000_000;
    //let _ = deposit_liquidity(&mut ctx_2, x_2_amount, y_2_amount);
    //// Swap parameters.
    //let swap_amount_2 = 0;
    //let slippage_2 = 1;
    //let swap_x_2 = 0;
    //zero_amount_swap(&mut ctx_2, swap_amount_2, slippage_2, swap_x_2);
    //println!(" ");

    // =================== SLIPPAGE PROTECTION SWAP TEST ==========
    // Swapping X for Y
    //let mut ctx_3 = setup_initialized_amm();
    //// Pool token amounts
    //let x_3_amount = 100_000_000;
    //let y_3_amount = 100_000;
    //let _ = deposit_liquidity(&mut ctx_3, x_3_amount, y_3_amount);
    //// Swap parameters.
    //// Swap amount is the amount of x to deposit for y
    //let swap_amount_3 = 50_000_000;
    //let slippage_3 = 30_000;
    //let swap_x_3 = 1; 
    //slippage_protected_swap(&mut ctx_3, swap_amount_3, slippage_3, swap_x_3);
    //println!(" ");

    //// Swapping Y for X
    //let mut ctx_4 = setup_initialized_amm();
    //// Pool token amounts
    //let x_4_amount = 1_000_100;
    //let y_4_amount = 100_000_000;
    //let _ = deposit_liquidity(&mut ctx_4, x_4_amount, y_4_amount);
    //// Swap parameters.
    //// Swap amount is the amount of y to deposit for x
    //let swap_amount_4 = 50_000_000; // y deposited to get x
    //let slippage_4 = 500;
    //let swap_x_4 = 1; // We are swapping toke Y for X, hence we set to 1
    //slippage_protected_swap(&mut ctx_4, swap_amount_4, slippage_4, swap_x_4);
    //println!(" ");

    //withdraw_liquidity(&mut ctx, &deposit_state);
}

proptest! {

    #[test]
    #[ignore]
    fn prop_no_negative_reserves(
        swap_amount in 1u64..900_000u64,
        swap_x in any::<bool>(),
    ) {
        let mut ctx = setup_initialized_amm();

        let x_amount = 1_000_000u64;
        let y_amount = 1_000_000u64;

        deposit_liquidity(&mut ctx, x_amount, y_amount);

        let slippage = 9_999u64;

        normal_swap(
            &mut ctx,
            swap_amount,
            slippage,
            if swap_x { 1 } else { 0 }
        );

    }

    #[test]
    fn prop_pool_reserve_behavior_under_random_swaps(
        initial_x in 1_000u64..10_000_000u64,
        initial_y in 1_000u64..10_000_000u64,
        swap_amount in 1u64..5_000_000u64,
        swap_x in any::<bool>(),
    ) {

        // Setup AMM with arbitrary reserve ratios
        let mut ctx = setup_initialized_amm();

        deposit_liquidity( &mut ctx, initial_x, initial_y );

        // Avoid impossible swaps that fully drain pool

        // Execute swap
        let amount_out = normal_swap(
            &mut ctx, swap_amount, 10_000,
            if swap_x { 1 } else { 0 },
        );

    }
}
