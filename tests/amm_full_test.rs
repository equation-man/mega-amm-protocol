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
#[ignore]
fn test_basic_swap() {
    // =================== NORMAL SWAP TEST(Balanced pool) ========================
    // Near zero slippage expected with 1:1 exchange rate on a balanced stable pool
    let mut ctx_1 = setup_initialized_amm();
    // Pool token amounts
    let x_1_amount = 1_000_000;
    let y_1_amount = 1_000_000;
    let _ = deposit_liquidity(&mut ctx_1, x_1_amount, y_1_amount);
    // Swap parameters.
    let swap_amount_1 = 10_000;
    let slippage_1 = 9_800;
    let swap_x_1 = 1;
    normal_swap(&mut ctx_1, swap_amount_1, slippage_1, swap_x_1);
    println!(" ");
}

#[test]
#[ignore]
fn test_withdrawing_liquidity() {
    let mut ctx_1 = setup_initialized_amm();
    // Pool token amounts
    let x_1_amount = 1_000_000;
    let y_1_amount = 1_000_000;
    let deposit_ctx = deposit_liquidity(&mut ctx_1, x_1_amount, y_1_amount);
    withdraw_liquidity(&mut ctx_1, &deposit_ctx);
}

// ================ PROPERTY TESTS ===============================
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

        normal_swap(&mut ctx, swap_amount, slippage, if swap_x { 1 } else { 0 });

    }

    #[test]
    #[ignore]
    fn prop_pool_reserve_behavior_under_random_swaps(
        initial_x in 1_000u64..10_000_000u64,
        initial_y in 1_000u64..10_000_000u64,
        swap_amount in 1u64..5_000_000u64,
        swap_x in any::<bool>(),
    ) {

        // Setup AMM with arbitrary reserve ratios
        let mut ctx = setup_initialized_amm();

        deposit_liquidity(&mut ctx, initial_x, initial_y);

        // Avoid impossible swaps that fully drain pool

        // Execute swap
        let amount_out = normal_swap(&mut ctx, swap_amount, 10_000, if swap_x { 1 } else { 0 });

    }

    #[test]
    #[ignore]
    fn prop_extreme_imbalance_pool_behavior(
        // Dominant reserve
        dominant_reserve in 1_000_000u64..50_000_000u64,
        // Weak reserve
        weak_reserve in 1_000u64..50_000u64,
        // Random swap size
        swap_amount in 1u64..5_000_000u64,
        // true  => x -> y
        // false => y -> x
        swap_x in any::<bool>(),
    ) {

        let mut ctx = setup_initialized_amm();

        // CASE 1:
        // Huge X reserve, tiny Y reserve
        // Trader swaps X -> Y
        // Pool should resist depletion of Y.
        let (initial_x, initial_y) = if swap_x {
            (dominant_reserve, weak_reserve)
        } else {
            // CASE 2:
            // Tiny X reserve, huge Y reserve
            // Trader swaps Y -> X
            // Pool should resist depletion of X.
            (weak_reserve, dominant_reserve)
        };

        // Deposit imbalanced liquidity
        deposit_liquidity(&mut ctx, initial_x, initial_y);

        // Execute swap
        normal_swap(&mut ctx, swap_amount, 10_000, if swap_x { 1 } else { 0 });
    }

    #[test]
    fn prop_normal_market_pool_behavior(
        // Normal market conditions
        // Pools are relatively balanced.
        // This simulates realistic stablecoin markets
        // near peg.
        base_liquidity in 1_000_000u64..50_000_000u64,
        // Small imbalance offset
        imbalance in 0u64..500_000u64,
        // Trade sizes are moderate relative to pool
        swap_amount in 1u64..2_000_000u64,
        // true  => x -> y
        // false => y -> x
        swap_x in any::<bool>(),
    ) {

        let mut ctx = setup_initialized_amm();

        // CASE 1:
        // Slightly more X liquidity
        // Trader swaps X -> Y
        let (initial_x, initial_y) = if swap_x {
            (base_liquidity + imbalance, base_liquidity)
        } else {
        // CASE 2:
        // Slightly more Y liquidity
        // Trader swaps Y -> X
            (base_liquidity, base_liquidity + imbalance)
        };

        // Deposit liquidity
        deposit_liquidity(&mut ctx, initial_x, initial_y);

        // Execute swap
        normal_swap(&mut ctx, swap_amount, 9_800, if swap_x { 1 } else { 0 });
    }
}
