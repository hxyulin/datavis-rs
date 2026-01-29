//! Test data builders for creating test objects

use datavis_rs::{Variable, VariableType};

/// Builder for creating test Variables
pub struct VariableBuilder {
    name: String,
    address: u64,
    var_type: VariableType,
}

impl VariableBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            address: 0x20000000,
            var_type: VariableType::U32,
        }
    }

    pub fn address(mut self, address: u64) -> Self {
        self.address = address;
        self
    }

    pub fn var_type(mut self, var_type: VariableType) -> Self {
        self.var_type = var_type;
        self
    }

    pub fn build(self) -> Variable {
        Variable::new(self.name, self.address, self.var_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_builder() {
        let var = VariableBuilder::new("test")
            .address(0x20001000)
            .var_type(VariableType::F32)
            .build();

        assert_eq!(var.name, "test");
        assert_eq!(var.address, 0x20001000);
        assert_eq!(var.var_type, VariableType::F32);
    }
}
