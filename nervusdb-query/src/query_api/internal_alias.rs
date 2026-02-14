use super::INTERNAL_PATH_ALIAS_PREFIX;

pub(super) fn alloc_internal_path_alias(next_anon_id: &mut u32) -> String {
    let alias = format!("{INTERNAL_PATH_ALIAS_PREFIX}{}", *next_anon_id);
    *next_anon_id += 1;
    alias
}

pub(super) fn is_internal_path_alias(alias: &str) -> bool {
    alias.starts_with(INTERNAL_PATH_ALIAS_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::{alloc_internal_path_alias, is_internal_path_alias};

    #[test]
    fn alloc_internal_path_alias_is_monotonic() {
        let mut next = 0;
        let a = alloc_internal_path_alias(&mut next);
        let b = alloc_internal_path_alias(&mut next);

        assert_eq!(a, "__nervus_internal_path_0");
        assert_eq!(b, "__nervus_internal_path_1");
        assert_eq!(next, 2);
    }

    #[test]
    fn detects_internal_path_alias_prefix() {
        assert!(is_internal_path_alias("__nervus_internal_path_42"));
        assert!(!is_internal_path_alias("p"));
        assert!(!is_internal_path_alias("__nervus_internal_path"));
    }
}
