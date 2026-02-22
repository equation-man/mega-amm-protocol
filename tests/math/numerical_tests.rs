use mega_amm_protocol::helpers::math_procs::numerical_ops::*;

// CONSTANT SUM TESTS
#[test]
fn test_constant_sum_basic() {
    let reserves = vec![100u128, 200u128, 300u128];
    let result = constant_sum(&reserves).unwrap();
    println!("Running the first constant product")
    assert_eq!(result, 600);
}

#[test]
fn test_constant_sum_empty() {
    let reserves: Vec<u128> = vec![];
    let result = constant_sum(&reserves).unwrap();
    assert_eq!(result, 0);
}

#[test]
fn test_constant_sum_overflow() {
    let reserves = vec![u128::MAX, 1u128];
    let result = constant_sum(&reserves);
    assert!(result.is_err());
}

// CONSTANT PRODUCT TESTS
#[test]
fn test_constant_product_basic() {
    let reserves = vec![10u128, 20u128];
    let result = constant_product(&reserves).unwrap();
    assert_eq!(result, 200);
}

#[test]
fn test_constant_product_with_one() {
    let reserves = vec![5u128];
    let result = constant_product(&reserves).unwrap();
    assert_eq!(result, 5);
}

#[test]
fn test_constant_product_overflow() {
    let reserves = vec![u128::MAX, 2u128];
    let result = constant_product(&reserves);
    assert!(result.is_err());
}

// TEST SLICE CONVERSION TO U128
#[test]
fn test_u128_conversion_success() {
    let input = vec![10u64, 20u64, 30u64];
    let mut output = vec![0u128; 3];

    u64_to_u128_inplace(&input, &mut output).unwrap();

    assert_eq!(output, vec![10u128, 20u128, 30u128]);
}

#[test]
fn test_u128_conversion_small_output_slice() {
    let input = vec![10u64, 20u64];
    let mut output = vec![0u128; 1];

    let result = u64_to_u128_inplace(&input, &mut output);
    assert!(result.is_err());
}

// SAFEGUARDED NEWTON SOLVER TEST.
#[test]
fn test_newton_solver_two_equal_reserves() {
    let reserves = vec![1_000_000u128, 1_000_000u128];
    let amp = 100u128;

    let d = safeguarded_newton_solver(&reserves, amp).unwrap();

    // For symmetric pool, D ≈ sum
    assert!(d >= 1_999_000 && d <= 2_001_000);
}

#[test]
fn test_newton_solver_imbalanced_pool() {
    let reserves = vec![2_000_000u128, 1_000_000u128];
    let amp = 100u128;

    let d = safeguarded_newton_solver(&reserves, amp).unwrap();

    // D must be between sum and n*max
    let sum = 3_000_000u128;
    let upper_bound = 2 * 2_000_000u128;

    assert!(d as u128 >= sum);
    assert!(d as u128 <= upper_bound);
}

#[test]
fn test_newton_solver_zero_reserve_error() {
    let reserves: Vec<u128> = vec![];
    let amp = 100u128;

    let result = safeguarded_newton_solver(&reserves, amp);
    assert!(result.is_err());
}
