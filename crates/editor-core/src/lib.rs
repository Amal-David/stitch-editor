//! Bootstrap-only compile target; editorial semantics belong to T-0007.

/// Version of the workspace/build contract, not a project-model schema.
pub const BOOTSTRAP_CONTRACT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::BOOTSTRAP_CONTRACT_VERSION;

    #[test]
    fn contract_version_is_explicit() {
        assert_eq!(BOOTSTRAP_CONTRACT_VERSION, 1);
    }
}
