use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Default, Clone)]
pub struct BudgetConfig {
    pub lints: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_lints() {
        let config = BudgetConfig::default();
        assert!(config.lints.is_none());
    }
}
