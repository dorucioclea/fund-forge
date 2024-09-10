use rust_decimal::prelude::{FromPrimitive, ToPrimitive, Zero};
use rust_decimal::Decimal;

/// Safely divides two f64 values using Decimal for precision.
/// Panics if the divisor is zero or if conversion fails.
pub fn divide_f64(dividend: f64, divisor: f64) -> f64 {
    if divisor == 0.0 {
        panic!("attempt to divide by zero");
    }
    let decimal_dividend = Decimal::from_f64(dividend).expect("Invalid dividend");
    let decimal_divisor = Decimal::from_f64(divisor).expect("Invalid divisor");

    let result = decimal_dividend / decimal_divisor;
    result.to_f64().expect("Error converting result to f64")
}

pub fn round_to_decimals(value: f64, decimals: u64) -> f64 { //todo fix using rust decimal, definitely not working properly
    let factor = 10f64.powi(decimals as i32);
    let rounded_value = (value * factor).round() / factor;
    let rounded_str = format!("{:.10}", rounded_value); // Convert to string with sufficient precision
    let truncated_str = rounded_str.trim_end_matches('0').trim_end_matches('.'); // Remove trailing zeros and decimal point if necessary
    truncated_str.parse::<f64>().unwrap_or(rounded_value) // Convert back to f64
}

pub fn round_to_tick_size(value: f64, tick_size: f64) -> f64 { //todo fix using rust decimal, definitely not working properly
    let rounded_value = (value / tick_size).round() * tick_size;
    let rounded_str = format!("{:.20}", rounded_value); // Convert to string with sufficient precision
    let truncated_str = rounded_str.trim_end_matches('0').trim_end_matches('.'); // Remove trailing zeros and decimal point if necessary
    truncated_str.parse::<f64>().unwrap_or(rounded_value) // Convert back to f64
}

/// Calculates the average of a vector of floating-point numbers using Decimal for high precision.
/// Skips NaN values and entries associated with zero quantity if applicable.
pub fn average_of_f64(values: &Vec<f64>) -> f64 {
    let mut total = Decimal::zero();
    let mut count = 0;

    for &price in values.iter() {
        // Here's an assumption that price should be skipped if it's NaN or somehow erroneous
        if price.is_nan() {
            continue; // Skip NaN values
        }
        // Convert float to Decimal, using zero as a fallback for conversion failure
        let decimal_price = Decimal::from_f64(price).unwrap_or(Decimal::zero());
        total += decimal_price;
        count += 1;
    }

    // Use the safe_divide function to perform division
    divide_decimal_by_usize(total, count)
}

/// Safely divides a Decimal by a usize, returning the result as a f64.
/// Panics if the denominator is zero.
pub fn divide_decimal_by_usize(numerator: Decimal, denominator: usize) -> f64 {
    if denominator == 0 {
        panic!("attempt to divide by zero");
    }
    let denominator_decimal = Decimal::from(denominator);
    let result = numerator / denominator_decimal;
    result.to_f64().unwrap_or_default() // Using default for f64 which is 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;
    use rust_decimal::Decimal;

    #[test]
    fn test_calculate_average_high_precision() {
        let prices = vec![0.00000001, 0.00000002, 0.00000003, 0.00000004];
        let average = average_of_f64(&prices);
        assert!(approx_eq!(
            f64,
            average,
            0.000000025,
            epsilon = 0.0000000001
        ));

        let prices_high_range = vec![100000.0, 200000.0, 300000.0];
        let average = average_of_f64(&prices_high_range);
        assert!(approx_eq!(f64, average, 200000.0, epsilon = 0.0001));
    }

    #[test]
    fn test_calculate_average_with_negative_values() {
        let prices = vec![-10.0, 20.0, -30.0, 40.0];
        let average = average_of_f64(&prices);
        assert!(approx_eq!(f64, average, 5.0, epsilon = 0.0001));
    }

    #[test]
    #[should_panic(expected = "attempt to divide by zero")]
    fn test_divide_decimal_by_usize_zero_denominator() {
        let numerator = Decimal::from(10);
        divide_decimal_by_usize(numerator, 0);
    }

    #[test]
    fn test_divide_decimal_by_usize_normal() {
        let numerator = Decimal::from_f64(50.0).unwrap();
        let result = divide_decimal_by_usize(numerator, 2);
        assert!(approx_eq!(f64, result, 25.0, epsilon = 0.00000001));
    }

    #[test]
    #[should_panic(expected = "attempt to divide by zero")]
    fn test_divide_f64_zero_divisor() {
        divide_f64(50.0, 0.0);
    }

    #[test]
    fn test_divide_f64_normal() {
        let result = divide_f64(50.0, 2.0);
        assert!(approx_eq!(f64, result, 25.0, epsilon = 0.00000001));
    }

    #[test]
    fn test_divide_f64_negative() {
        let result = divide_f64(-100.0, 4.0);
        assert!(approx_eq!(f64, result, -25.0, epsilon = 0.00000001));
    }
}
