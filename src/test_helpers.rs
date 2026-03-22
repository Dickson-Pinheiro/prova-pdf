//! Shared test helpers — font fixtures and common setup functions.
//!
//! Eliminates duplication of `DEJAVU` bytes, `make_resolver_and_rules()`,
//! and other test utilities that were previously copy-pasted across 12+ test modules.

#[cfg(test)]
pub mod fixtures {
    use crate::fonts::data::{FontData, FontFamily};
    use crate::fonts::registry::{FontRegistry, FontRules};

    /// DejaVu Sans font bytes — shared test fixture.
    pub const DEJAVU: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

    /// Alias for modules that historically used `DEJAVU_SANS`.
    pub const DEJAVU_SANS: &[u8] = DEJAVU;

    /// Create a minimal FontRegistry + FontRules with DejaVu Sans as the "body" family.
    pub fn make_resolver_and_rules() -> (FontRegistry, FontRules) {
        let mut reg = FontRegistry::new();
        reg.add_family("body", FontFamily::new(FontData::from_bytes(DEJAVU).unwrap()));
        (reg, FontRules::default())
    }
}
