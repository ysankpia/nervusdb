use nervusdb_query::prepare;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn does_not_crash_on_random_string_parsing(s in "\\PC*") {
        // We just want to ensure it doesn't panic.
        // It's expected to error on almost all inputs.
        let _ = prepare(&s);
    }
}
