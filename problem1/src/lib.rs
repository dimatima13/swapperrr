//! Zero-padding utility for numbers in strings.

use regex::Regex;

/// Pads all whole numbers in a string with leading zeros to the specified length.
///
/// # Arguments
/// * `input` - The input string containing numbers to pad
/// * `pad_length` - The desired length for each number after padding
///
/// # Examples
/// ```
/// use problem1::pad_numbers;
/// 
/// assert_eq!(pad_numbers("James Bond 7", 3), "James Bond 007");
/// assert_eq!(pad_numbers("PI=3.14", 2), "PI=03.14");
/// ```
pub fn pad_numbers(input: &str, pad_length: usize) -> String {
    let re = Regex::new(r"\d+").unwrap();
    
    re.replace_all(input, |caps: &regex::Captures| {
        let num = &caps[0];
        format!("{:0>width$}", num, width = pad_length)
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_james_bond() {
        assert_eq!(pad_numbers("James Bond 7", 3), "James Bond 007");
    }

    #[test]
    fn test_pi() {
        assert_eq!(pad_numbers("PI=3.14", 2), "PI=03.14");
    }

    #[test]
    fn test_time_3_13() {
        assert_eq!(pad_numbers("It's 3:13pm", 2), "It's 03:13pm");
    }

    #[test]
    fn test_time_12_13() {
        assert_eq!(pad_numbers("It's 12:13pm", 2), "It's 12:13pm");
    }

    #[test]
    fn test_99ur1337() {
        assert_eq!(pad_numbers("99UR1337", 6), "000099UR001337");
    }

    #[test]
    fn test_no_numbers() {
        assert_eq!(pad_numbers("Hello World", 5), "Hello World");
    }

    #[test]
    fn test_single_digit() {
        assert_eq!(pad_numbers("Room 5", 4), "Room 0005");
    }

    #[test]
    fn test_multiple_numbers() {
        assert_eq!(pad_numbers("1 plus 2 equals 3", 2), "01 plus 02 equals 03");
    }

    #[test]
    fn test_already_padded() {
        assert_eq!(pad_numbers("007", 3), "007");
    }

    #[test]
    fn test_mixed_padding() {
        assert_eq!(pad_numbers("Agent 007 in room 5", 3), "Agent 007 in room 005");
    }

    // Corner cases
    #[test]
    fn test_empty_string() {
        assert_eq!(pad_numbers("", 5), "");
    }

    #[test]
    fn test_zero_padding() {
        assert_eq!(pad_numbers("Room 42", 0), "Room 42");
    }

    #[test]
    fn test_decimal_numbers() {
        assert_eq!(pad_numbers("3.14159", 4), "0003.14159");
        assert_eq!(pad_numbers("Version 2.0.1", 3), "Version 002.000.001");
    }

    #[test]
    fn test_negative_numbers() {
        assert_eq!(pad_numbers("-42", 4), "-0042");
        assert_eq!(pad_numbers("Temperature: -5 degrees", 3), "Temperature: -005 degrees");
    }

    #[test]
    fn test_numbers_in_words() {
        assert_eq!(pad_numbers("abc123def", 5), "abc00123def");
        assert_eq!(pad_numbers("test123test456test", 4), "test0123test0456test");
    }

    #[test]
    fn test_unicode() {
        assert_eq!(pad_numbers("Комната 5", 3), "Комната 005");
        assert_eq!(pad_numbers("🎯 Score: 42", 4), "🎯 Score: 0042");
    }

    #[test]
    fn test_scientific_notation() {
        assert_eq!(pad_numbers("1e10", 3), "001e010");
        assert_eq!(pad_numbers("3.14e-5", 4), "0003.0014e-0005");
    }

    #[test]
    fn test_very_long_numbers() {
        assert_eq!(pad_numbers("12345678", 3), "12345678");
        assert_eq!(pad_numbers("12345678", 10), "0012345678");
    }

    #[test]
    fn test_special_cases_from_examples() {
        // All standalone numbers should be padded
        assert_eq!(pad_numbers("PI=3 and 14", 2), "PI=03 and 14");
    }

    #[test]
    fn test_edge_cases() {
        // Padding length of 1
        assert_eq!(pad_numbers("1 2 3", 1), "1 2 3");
        
        // Only zeros
        assert_eq!(pad_numbers("000", 5), "00000");
        
        // Numbers at start and end
        assert_eq!(pad_numbers("5 words 10", 3), "005 words 010");
        
        // Consecutive numbers
        assert_eq!(pad_numbers("123456", 2), "123456");
        
        // Numbers with special characters
        assert_eq!(pad_numbers("$100 + €50 = £150", 4), "$0100 + €0050 = £0150");
        
        // Tab and newline
        assert_eq!(pad_numbers("Line1: 5\nLine2: 10", 3), "Line001: 005\nLine002: 010");
        assert_eq!(pad_numbers("Col1\t5\tCol2\t10", 2), "Col01\t05\tCol02\t10");
    }

    #[test]
    fn test_original_examples_exact() {
        // Exact examples from the problem statement
        assert_eq!(pad_numbers("James Bond 7", 3), "James Bond 007");
        assert_eq!(pad_numbers("PI=3.14", 2), "PI=03.14");
        assert_eq!(pad_numbers("It's 3:13pm", 2), "It's 03:13pm");
        assert_eq!(pad_numbers("It's 12:13pm", 2), "It's 12:13pm");
        assert_eq!(pad_numbers("99UR1337", 6), "000099UR001337");
    }
}