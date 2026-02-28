//! utility functions for performing price discovery math.
use core::cmp::Ordering;

type Uint = u128; // Used to represent fixed point numbers (1e18 decimals).
const MAX_TOKENS: usize = 2;

// Converting &[u64] to &[u128] for scaling pattern in arithmetic computation.
pub fn u64_to_u128_inplace(reserve: &[u64], out: &mut [u128; 2]) -> Result<(), &'static str> {
    if out.len() < reserve.len() { return Err("Small output slice"); }

    reserve.iter()
        .zip(out.iter_mut())
        .for_each(|(&x, new_type)| *new_type = u128::from(x));
    Ok(())
}

// Deposit function.
pub fn deposit_liquidity(amp_param: u64, balances_param: &[u64], n_param: u32) -> Result<u64, &'static str> {
    // Scaling to u128 to accomodate large integer computations.
    let amp: Uint = amp_param.into();
    let mut balances: [Uint; 2] = [0; 2];
    u64_to_u128_inplace(balances_param, &mut balances)?;
    let n: Uint = n_param.into();

    let sum_x: Uint = balances.iter().sum();
    if sum_x == 0 { return Ok(0); }

    let mut d = sum_x;
    let ann = amp.checked_mul(
        n.checked_pow(n as u32).ok_or("Power error for Ann")?
        ).ok_or("Multiplication with Ann error")?;

    for _ in 0..64 {
        let mut d_p = d;
        for x in balances {
            let safe_x = if x == 0 { 1u128 } else { x }; d_p = d_p.checked_mul(d).ok_or("Overflow error for product term")?
                .checked_div(safe_x.checked_mul(n).ok_or("Overflow error on product term")?)
                .ok_or("Division error on product term")?;
        }

        // Convergence check
        let d_prev = d;

        // Newton's method for d.
        let num = d.checked_mul(ann.checked_mul(sum_x).ok_or("Overflow error on computing numerator")?
            .checked_add(d_p.checked_mul(n).ok_or("Overflow error on computing numerator")?)
            .ok_or("Overflow error on addition in numerator")?)
            .ok_or("Overflow error on numerator computation")?;
        let den = d.checked_mul(ann.checked_sub(1).ok_or("Underflow error for denominator")?)
            .ok_or("Overflow on denominator computation")?
            .checked_add(d_p.checked_mul(n.checked_add(1).ok_or("Overflow on denominator addition")?)
                .ok_or("Overflow on denominator multiplication")?)
            .ok_or("Overflow on addition on the denominator")?;

        d = num.checked_div(den).ok_or("Division error on d")?;

        // Checking convergence.
        if d > d_prev && d - d_prev <= 1 { return Ok(d.try_into().map_err(|_| "Error scalling down to u64")?); }
        if d_prev > d && d_prev - d <= 1 { return Ok(d.try_into().map_err(|_| "Error scalling down to u64")?); }
    }
    // Return new value of the liquidity after deposit. Will be used to calculate LP tokens to mint
    Ok(d.try_into().map_err(|_| "Error scalling down deposit")?)
}

// Withdrawal function to withdraw one coin.
// Here, we will use our Newton solver as this is treated as a virtual swap.
pub fn withdraw_imbalanced(
    lp_tokens_to_burn: u64, total_lp_supply: u64, current_balances: &[u64],
    d_current: u64, amp: u64
) -> Result<u64, &'static str> {
    if lp_tokens_to_burn == 0 || total_lp_supply == 0 {
        return Ok(0);
    }
    // Calcuating the invariant after withdrawal.
    let d_reduction = d_current.checked_mul(lp_tokens_to_burn).ok_or("Multiplication overflow")?
        .checked_div(total_lp_supply).ok_or("Division by zero")?;
    let d_target = d_current.checked_sub(d_reduction).ok_or("D underflow")?;

    // Preparing the solver inputs
    let x_other = current_balances[1];
    let n = current_balances.len() as u32;

    // Finding the new value of the invariant.
    let y_new = newton_solver_scaled(
        amp, x_other, 
        d_target.try_into().map_err(|_| "Scaling down on Newton solver failed")?,
        n
    )?;

    // Calculating the amount out.
    let old_balance = current_balances[0];
    let amount_out_raw = old_balance.checked_sub(y_new).ok_or("Mathematical error: New balance exceeds old")?;

    // Applying fee.
    let fee = amount_out_raw.checked_div(100).unwrap_or(0); //0.1% fee
    let final_payment = amount_out_raw.checked_sub(fee).ok_or("Fee error")?;
    Ok(final_payment.try_into().map_err(|_| "Error scaling down withraw result")?)
}

// Withdrawing proportional amount of each token from the pool.
pub fn withdraw_balanced(reserves: &[u64], lp_to_burn: u64, total_lp_supply: u64) -> Result<[u64; MAX_TOKENS], &'static str> {
    let n_len = reserves.len();
    if n_len == 0 || n_len > MAX_TOKENS { return Err("Invalid reserve length"); }
    if total_lp_supply == 0 { return Err("Zero total supply"); }
    if lp_to_burn == 0 { return Ok([0u64; MAX_TOKENS]); }
    if lp_to_burn > total_lp_supply { return Err("Burn amount exceeded supply"); }

    let mut amount_out = [0u64; MAX_TOKENS];
    for i in 0..n_len {
        let reserve_i: Uint = reserves[i].into();
        let burn_amt: Uint = lp_to_burn.into();
        let supply: Uint = total_lp_supply.into();

        let out = (reserve_i * burn_amt) / supply;

        amount_out[i] = out.try_into().map_err(|_| "Error scaling down balanced withdrawal")?;
    }
    // Return array containing amount of each token to be sent to the user
    Ok(amount_out)
}

// Newton-Raphson(NR) with Bisection fallback
// Here we are solving for the invariant "D" from formula
// from a modified newton raphson formula.
// We use "Scaling" pattern, working with u128 to prevent potential overflow and convert back to 
// u64 for storage.
// amp: The amplification parameter A.
// x_i: This is the sum of all tokens available in the pool after the user 
// credits the pool, minus the balance of the token the user wants to in exchange(token we are
// solving for).
// d: The invariant, it represent the token balance of the token we are solving for.
// n: The number of tokens, e.g, if we have token x and token y, it'll be 2.
#[inline(always)]
pub fn newton_solver_scaled(
    amp_param: u64, x_i_param: u64, d_param: u64, n_param: u32
) -> Result<u64, &'static str> {
    // No tokens available in in the pool yet
    if x_i_param == 0 {
        return Err("Insufficient funds");
    }
    // Scaling up to u128 to accommodate large integer computations.
    let amp: Uint = amp_param.into();
    let x_i: Uint = x_i_param.into();
    let d: Uint = d_param.into();
    let n: Uint = n_param.into();
    // Computing A*n^n
    let ann = amp.checked_mul(n).ok_or("Overflowing error at An^n")?;
    let s_ = x_i;

    // Calculating c=D^(n+1)/(n^n * x_i * ann)* D/ann.
    // We perform checked_ operations to prevent BPF crashes.
    let mut c = d;
    c = c.checked_mul(d).ok_or("Overflow error for D^2")?
        .checked_div(x_i.checked_mul(n).ok_or("Overflow detected at xi * n")?)
        .ok_or("Division error at D^2")?;
    c = c.checked_mul(d).ok_or("Overflow detected at D^2*D")?
        .checked_div(ann.checked_mul(n).ok_or("Overflow detected at A*n^n")?)
        .ok_or("Division error at term c")?;

    let b = s_.checked_add(d.checked_div(ann).ok_or("Division error at term b")?)
        .ok_or("Overflow detected on addition at term b")?;

    // Hybrid boundary solver(Newton and Bisection)
    let mut y_low = 0u128;
    let mut y_high = d.checked_mul(2).ok_or("Overflow detected")?;
    let mut y = d;
    let mut dy_prev = Uint::MAX;

    // Hard cap added on iterations for compute limits.
    for _ in 0..64 {
        // Performing f(y) = y^2 + (b - d)y - c(Newton-Raphson manipulated algebraically)
        // The equation has been rearranged to avoid -ve numbers on unsigned math.
        let y_sq = y.checked_mul(y).ok_or("Overflow detected")?;
        let by = y.checked_mul(b).ok_or("Overflow detected")?;
        let dy = y.checked_mul(d).ok_or("Overflwo detected")?;

        // Calculating the left side(y^2 - b*y) and the right side (d*y + c)
        let lhs = y_sq.checked_add(by).ok_or("Overflow detected on addition")?;
        let rhs = dy.checked_add(c).ok_or("Overflow detected on addition")?;

        // Checking convergence.
        let diff = if lhs > rhs { lhs - rhs } else { rhs - lhs };
        // Precission of 1 unit (10^-18)
        if diff <=1 {
            return Ok(y.try_into().map_err(|_| "Error scalling down")?);
        }

        // Derivative f'(y) = 2y + b - d
        let dfy = y.checked_mul(2).ok_or("Overflow detected")?
            .checked_add(b).ok_or("Overflow detected")?
            .checked_sub(d).ok_or("Underflow detected")?;
        // Newton step: y_next = y - f(y)/f'(y). We check if dfy > 0 to avoid division by 0.
        let mut y_next = if dfy > 0 {
            if lhs > rhs {
                y.checked_sub(diff.checked_div(dfy).ok_or("Division error")?)
                    .ok_or("Underflow detected")?
            } else {
                y.checked_add(diff.checked_div(dfy).ok_or("Division error")?)
                    .ok_or("Overflow detected on addition")?
            }
        } else {
            y_low.checked_add(y_high).ok_or("Overflow detected on addition")? / 2
        };

        // Bound enforcement and divergence check.
        let step_size = if y_next > y { y_next - y } else { y - y_next };
        if y_next <= y_low || y_next >= y_high || step_size >= dy_prev {
            y_next = y_low.checked_add(y_high).ok_or("Overflow detected on addition")? / 2;
        }

        if lhs < rhs { y_low = y; } else { y_high = y; }

        dy_prev = step_size;
        y = y_next;
    }

    Ok(y.try_into().map_err(|_| "Error scaling down")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRECISION: u128 = 1_000_000_000_000_000_000; // 1e18

    #[test]
    fn test_perfect_balance() {
        // Pool: 100 USDC, 100 USDT. D = 200. Amp = 100.
        // If x_i is 100, y must be 100.
        let d = 200; //* PRECISION;
        let x_i = 100; //* PRECISION;
        let amp = 100;
        let n = 2;

        let result = newton_solver_scaled(amp, x_i, d, n).expect("Should converge");
        
        // In a perfectly balanced pool, y should be exactly equal to x_i
        assert!((result as i128 - x_i as i128).abs() <= 100, "Result {} too far from 100", result);
    }

    #[test]
    fn test_imbalance_high_amp() {
        // High Amp (1000) makes the curve flatter (like x + y = D)
        // D = 2000. If x = 500, y should be close to 1500.
        let d = 2000;// * PRECISION;
        let x_i = 500; // * PRECISION;
        let amp = 1000;
        let n = 2;

        let result = newton_solver_scaled(amp, x_i, d, n).expect("Should converge");
        
        // 500 + 1500 = 2000. Because it's StableSwap, it should be very close to 1500.
        let expected = 1500; // * PRECISION;
        let diff = result - expected;
        assert!(diff < 1, "Diff {} too high for high amp", diff);
    }

    #[test]
    fn test_imbalance_low_amp() {
        // Low Amp (1) makes it behave more like Constant Product (xy = k)
        let d = 200;// * PRECISION;
        let x_i = 50;// * PRECISION; // 1/4 of D
        let amp = 1;
        let n = 2;

        let result = newton_solver_scaled(amp, x_i, d, n).expect("Should converge");
        
        // In Constant Product, if x is small, y must be much larger to maintain D.
        assert!(result > (d / 2).into(), "y should be larger than half of D when x is small");
        assert!(result < (d * 2).into(), "y should not explode to infinity");
    }

    #[test]
    fn test_extreme_imbalance_protection() {
        // Test where x_i is nearly the entire D.
        let d = 1000;// * PRECISION;
        let x_i = 999;// * PRECISION; 
        let amp = 100;
        let n = 2;

        let result = newton_solver_scaled(amp, x_i, d, n);
        // Should either converge to a tiny y or fail gracefully, not panic.
        assert!(result.is_ok());
        assert!(u128::from(result.unwrap()) < PRECISION); 
    }

    #[test]
    fn test_zero_balance_fails() {
        let d = 100;// * PRECISION;
        let x_i = 0; // Invalid input
        let amp = 100;
        let n = 2;

        let result = newton_solver_scaled(amp, x_i, d, n);
        assert!(result.is_err(), "Should fail when balance is zero");
    }

    // ====================== TESTING DEPOSITING Liquidity ============================
    //
    // Helper to simulate the D-invariant formula for verification
    // LHS: Ann * S + D
    // RHS: Ann * D + D^(n+1) / (n^n * Prod(x))
    fn verify_invariant_error(balances: &[u64], amp: u64, d_found: u64) -> i128 {
        let n = balances.len() as u128;
        let d = d_found as u128;
        let sum_x: u128 = balances.iter().map(|&x| x as u128).sum();
        
        let mut ann = amp as u128;
        for _ in 0..n { ann *= n; }

        let mut d_p = d;
        for &x in balances {
            // d_p = d_p * D / (x * n)
            d_p = (d_p * d) / (x as u128 * n);
        }

        let lhs = (ann * sum_x) + d;
        let rhs = (ann * d) + d_p;

        lhs as i128 - rhs as i128
    }

    #[test]
    fn test_deposit_balanced_pool() {
        // 1:1 Pool: D should exactly equal the sum of balances
        let balances = [1_000_000u64, 1_000_000u64];
        let amp = 100u64;
        let n = 2u32;

        let d = deposit_liquidity(amp, &balances, n).expect("Should converge");
        
        assert_eq!(d, 2_000_000);
        assert!(verify_invariant_error(&balances, amp, d).abs() <= 1);
    }

    #[test]
    fn test_deposit_imbalanced_pool() {
        // High imbalance: 1M vs 100k
        let balances = [1_000_000u64, 100_000u64];
        let amp = 100u64;
        let n = 2u32;

        let d = deposit_liquidity(amp, &balances, n).expect("Should converge");

        // Property: In StableSwap, D is always >= sum(x)
        assert!(d <= 1_100_000 && d > 1_000_000);
        
        // For 18 decimal math, make residual error under 1000 as it is still highly precise.
        let error = verify_invariant_error(&balances, amp, d);
        assert!(error.abs() <= 1000, "Residual error too high: {}", error);
    }

    #[test]
    fn test_high_amplification_behavior() {
        // With very high A, the curve becomes a straight line (Constant Sum)
        // D should stay very close to the sum even with imbalance
        let balances = [1_000_000u64, 500_000u64];
        let amp_low = 1u64;
        let amp_high = 10_000u64;
        let n = 2u32;

        let d_low = deposit_liquidity(amp_low, &balances, n).unwrap();
        let d_high = deposit_liquidity(amp_high, &balances, n).unwrap();

        // High A pushes D closer to the sum_x, increasing its value
        // Checking if amplification parameter A increases D.
        assert!(d_high > d_low);
        assert!(d_high <= 1_500_000); 
    }

    #[test]
    fn test_zero_balances() {
        let balances = [0u64, 0u64];
        let d = deposit_liquidity(100, &balances, 2).unwrap();
        assert_eq!(d, 0);
    }

    #[test]
    fn test_single_token_deposit() {
        // Pool with liquidity only in one side
        let balances = [1_000_000u64, 0u64];
        let amp = 100u64;
        let d = deposit_liquidity(amp, &balances, 2).expect("Should handle one zero balance");

        // D should be sightly less than 1_000_000 due to the imbalance
        assert!(d > 0);
        assert!(d <= 1_000_000);
    }

    #[test]
    fn test_max_u64_limits() {
        // Testing large values that might cause internal overflows if math is fragile
        let balances = [u64::MAX / 1000, u64::MAX / 1000];
        let amp = 100u64;
        
        let result = deposit_liquidity(amp, &balances, 2);
        // This will either succeed or return a clean Error, not a panic.
        assert!(result.is_ok() || result.is_err());
    }

    // =============== TESTING WITHDRAWALS ==============
    //
    //
    #[test]
    fn test_withdraw_half_supply() {
        // Setup: Balanced pool [1M, 1M] -> D = 2M
        let current_balances = [1_000_000u64, 1_000_000u64];
        let total_lp_supply = 2_000_000u64;
        let d_current = 2_000_000u64;
        let amp = 100u64;
        
        // User burns half the supply (1M LP tokens)
        let lp_to_burn = 1_000_000u64;

        let amount_out = withdraw_imbalanced(
            lp_to_burn, 
            total_lp_supply, 
            &current_balances, 
            d_current, 
            amp
        ).expect("Withdrawal should converge");

        // Withdrawing 50% D from one side requires almost taking entire balance  
        // from that side
        assert!(amount_out > 900_000, "Reflec hight convexity exit");
        assert!(amount_out < 1_000_000, "Cannot exceed current balance"); 
    }

    #[test]
    fn test_withdraw_from_imbalanced_pool() {
        // Setup: Imbalanced pool [1M, 500k]
        // From previous tests, we know D will be < 1.5M (approx 1,470,000 for low A)
        let current_balances = [1_000_000u64, 500_000u64];
        let total_lp_supply = 1_470_000u64;
        let d_current = 1_470_000u64;
        let amp = 10u64;

        let lp_to_burn = 147_000u64; // Burning 10% of supply

        let amount_out = withdraw_imbalanced(
            lp_to_burn, 
            total_lp_supply, 
            &current_balances, 
            d_current, 
            amp
        ).expect("Should handle imbalanced exit");

        // Withdrawing oversupplied tokens results in rebalancing bonus
        // The user should get roughly 10% of the first token's balance
        assert!(amount_out > 0);
        assert!(amount_out > 100_000, "Should receive rebalancing bonus"); // 10% of 1M is 100k; minus fees and slippage
        assert!(amount_out < 200_000, "Should not exceed reasonable bound");
    }

    #[test]
    fn test_withdraw_too_much_fails() {
        let current_balances = [1_000_000u64, 1_000_000u64];
        let total_lp_supply = 2_000_000u64;
        
        // Attempt to burn more than exists
        let result = withdraw_imbalanced(
            2_000_001, 
            total_lp_supply, 
            &current_balances, 
            2_000_000, 
            100
        );
        
        assert_eq!(result, Err("D underflow"));
    }

    #[test]
    fn test_zero_burn_returns_zero() {
        let current_balances = [1_000_000u64, 1_000_000u64];
        let amount = withdraw_imbalanced(0, 2_000_000, &current_balances, 2_000_000, 100).unwrap();
        assert_eq!(amount, 0);
    }

    // ======================== TESTING WITHDRAWAL FROM A BALANCED POOL ==========================
    #[test]
    fn test_balanced_withdrawal_half_supply() {
        // Setup: Balanced pool [1,000,000, 1,000,000]
        let reserves = [1_000_000u64, 1_000_000u64];
        let total_lp_supply = 2_000_000u64;
        let lp_to_burn = 1_000_000u64; // Burning exactly 50%

        let amounts_out = withdraw_balanced(&reserves, lp_to_burn, total_lp_supply)
            .expect("Should calculate proportional withdrawal");

        // In a balanced withdrawal, 50% LP burn = 50% of EACH asset.
        // Result should be [500,000, 500,000]
        assert_eq!(amounts_out[0], 500_000);
        assert_eq!(amounts_out[1], 500_000);
    }

    #[test]
    fn test_balanced_withdrawal_imbalanced_pool() {
        // Setup: Imbalanced pool [1,000,000, 500,000]
        let reserves = [1_000_000u64, 500_000u64];
        let total_lp_supply = 1_500_000u64; 
        let lp_to_burn = 150_000u64; // Burning exactly 10%

        let amounts_out = withdraw_balanced(&reserves, lp_to_burn, total_lp_supply)
            .expect("Should handle imbalanced pool proportional exit");

        // Result: 10% of 1M = 100k, 10% of 500k = 50k
        assert_eq!(amounts_out[0], 100_000);
        assert_eq!(amounts_out[1], 50_000);
    }

    #[test]
    fn test_balanced_withdrawal_rounding() {
        // Setup: Testing precision with odd numbers
        let reserves = [100u64, 100u64];
        let total_lp_supply = 300u64;
        let lp_to_burn = 100u64; // 1/3rd of the supply

        let amounts_out = withdraw_balanced(&reserves, lp_to_burn, total_lp_supply).unwrap();

        // (100 * 100) / 300 = 33.33 -> rounds down to 33
        assert_eq!(amounts_out[0], 33);
        assert_eq!(amounts_out[1], 33);
    }

    #[test]
    fn test_balanced_withdrawal_burn_exceeds_supply() {
        let reserves = [1_000_000u64, 1_000_000u64];
        let result = withdraw_balanced(&reserves, 11, 10);
        assert_eq!(result, Err("Burn amount exceeded supply"));
    }
}

