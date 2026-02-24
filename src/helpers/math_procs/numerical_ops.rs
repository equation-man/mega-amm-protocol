//! utility functions for performing price discovery math.
use core::cmp::Ordering;

// Calculating the contant product, sum of x_i.
pub fn constant_sum(reserves: &[u64]) -> Result<u64, &'static str> {
    reserves.iter().try_fold(0u64, |acc, &x| {
        acc.checked_add(x as u64).ok_or("Constant sum overflow")
    })
}

// Calculating the constant product, product of x_i.
pub fn constant_product(reserves: &[u128]) -> Result<u128, &'static str> {
    reserves.iter().try_fold(1u128, |acc, &x| {
        acc.checked_mul(x as u128).ok_or("Constant product overflow")
    })
}

// Converting &[u64] to &[u128] for scaling pattern in arithmetic computation.
pub fn u64_to_u128_inplace(
    reserve: &[u64], out: &mut [u128; 2]
) -> Result<(), &'static str> {
    if out.len() < reserve.len() { return Err("Small output slice"); }

    reserve.iter()
        .zip(out.iter_mut())
        .for_each(|(&x, new_type)| *new_type = u128::from(x));
    Ok(())
}

// Newton-Raphson(NR) with Bisection fallback
// Here we are solving for the invariant "D" from formula
// from a modified newton raphson formula.
// We use "Scaling" pattern, working with u128 to prevent potential overflow and convert back to 
// u64 for storage.

type Uint = u128; // Used to represent fixed point numbers (1e18 decimals).
const ONE: usize = 8;

// amp: The amplification parameter A.
// x_i: This is the sum of all tokens available in the pool after the user 
// credits the pool, minus the balance of the token the user wants to in exchange(token we are
// solving for).
// d: The invariant, it represent the token balance of the token we are solving for.
// n: The number of tokens, e.g, if we have token x and token y, it'll be 2.
pub fn newton_solver_scaled(
    amp_param: u64, x_i_param: u64, d_param: u64, n_param: u8
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
        println!("The results for y and x are {} {}", result, x_i);
        
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
        println!("The result when balance is 0 is {:?}", result);
        assert!(result.is_err(), "Should fail when balance is zero");
    }
}

