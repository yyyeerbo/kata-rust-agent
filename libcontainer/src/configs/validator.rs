use libcontainer::configs::Config;

pub trait Validator {
	fn validate(&self, config: &Config) -> Result<()> {
		Ok(())
	}
}

pub struct ConfigValidator {
}

impl Validator for ConfigValidator {
}

impl ConfigValidator {
	fn new() -> Self {
		ConfigValidator { }
	}
}
