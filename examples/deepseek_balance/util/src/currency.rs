//! Currency display: a code becomes the symbol a reader recognises.

/// Currency code to symbol, falling back to the code itself.
pub fn symbol(currency: &str) -> String {
    match currency {
        "CNY" | "RMB" => "¥".to_string(),
        "USD" => "$".to_string(),
        "EUR" => "€".to_string(),
        "HKD" => "HK$".to_string(),
        // An unknown code needs a separator, or `JPY1000` reads as one token.
        other => format!("{other} "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_get_symbols_and_unknown_ones_a_separator() {
        assert_eq!(symbol("CNY"), "¥");
        assert_eq!(symbol("RMB"), "¥");
        assert_eq!(symbol("USD"), "$");
        assert_eq!(symbol("EUR"), "€");
        assert_eq!(symbol("HKD"), "HK$");
        assert_eq!(symbol("JPY"), "JPY ");
    }
}
