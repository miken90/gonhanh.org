//! Shortcut Table - Abbreviation expansion
//!
//! Allows users to define shortcuts like "vn" → "Việt Nam"
//! Shortcuts can be specific to input methods (Telex/VNI) or apply to all.

use std::collections::HashMap;

/// Input method that shortcut applies to
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InputMethod {
    /// Apply to all input methods
    #[default]
    All,
    /// Apply only to Telex
    Telex,
    /// Apply only to VNI
    Vni,
}

/// Trigger condition for shortcut
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerCondition {
    /// Trigger immediately when buffer matches
    Immediate,
    /// Trigger when word boundary (space, punctuation) is pressed
    OnWordBoundary,
}

/// Case handling mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaseMode {
    /// Keep replacement exactly as defined
    Exact,
    /// Match case of trigger: "VN" → "VIỆT NAM", "vn" → "Việt Nam"
    MatchCase,
}

/// A single shortcut entry
#[derive(Debug, Clone)]
pub struct Shortcut {
    /// Trigger string (lowercase for matching)
    pub trigger: String,
    /// Replacement text
    pub replacement: String,
    /// When to trigger
    pub condition: TriggerCondition,
    /// How to handle case
    pub case_mode: CaseMode,
    /// Whether this shortcut is enabled
    pub enabled: bool,
    /// Which input method this shortcut applies to
    pub input_method: InputMethod,
}

impl Shortcut {
    /// Create a new shortcut with word boundary trigger (applies to all input methods)
    pub fn new(trigger: &str, replacement: &str) -> Self {
        Self {
            trigger: trigger.to_lowercase(),
            replacement: replacement.to_string(),
            condition: TriggerCondition::OnWordBoundary,
            case_mode: CaseMode::MatchCase,
            enabled: true,
            input_method: InputMethod::All,
        }
    }

    /// Create an immediate trigger shortcut (applies to all input methods)
    pub fn immediate(trigger: &str, replacement: &str) -> Self {
        Self {
            trigger: trigger.to_lowercase(),
            replacement: replacement.to_string(),
            condition: TriggerCondition::Immediate,
            case_mode: CaseMode::Exact,
            enabled: true,
            input_method: InputMethod::All,
        }
    }

    /// Create a Telex-specific shortcut with immediate trigger
    pub fn telex(trigger: &str, replacement: &str) -> Self {
        Self {
            trigger: trigger.to_lowercase(),
            replacement: replacement.to_string(),
            condition: TriggerCondition::Immediate,
            case_mode: CaseMode::Exact,
            enabled: true,
            input_method: InputMethod::Telex,
        }
    }

    /// Create a VNI-specific shortcut with immediate trigger
    pub fn vni(trigger: &str, replacement: &str) -> Self {
        Self {
            trigger: trigger.to_lowercase(),
            replacement: replacement.to_string(),
            condition: TriggerCondition::Immediate,
            case_mode: CaseMode::Exact,
            enabled: true,
            input_method: InputMethod::Vni,
        }
    }

    /// Set the input method for this shortcut
    pub fn for_method(mut self, method: InputMethod) -> Self {
        self.input_method = method;
        self
    }

    /// Check if shortcut applies to given input method
    ///
    /// - If shortcut is for `All`: matches any method
    /// - If shortcut is for `Telex`: matches `Telex` or `All` query
    /// - If shortcut is for `Vni`: matches `Vni` or `All` query
    pub fn applies_to(&self, query_method: InputMethod) -> bool {
        match self.input_method {
            // Shortcut for All → matches any query
            InputMethod::All => true,
            // Shortcut for specific method → matches if query is same method OR query is All
            InputMethod::Telex => query_method == InputMethod::Telex || query_method == InputMethod::All,
            InputMethod::Vni => query_method == InputMethod::Vni || query_method == InputMethod::All,
        }
    }
}

/// Shortcut match result
#[derive(Debug)]
pub struct ShortcutMatch {
    /// Number of characters to backspace
    pub backspace_count: usize,
    /// Replacement text to output
    pub output: String,
    /// Whether to include the trigger key in output
    pub include_trigger_key: bool,
}

/// Shortcut table manager
#[derive(Debug, Default)]
pub struct ShortcutTable {
    /// Shortcuts indexed by trigger (lowercase)
    shortcuts: HashMap<String, Shortcut>,
    /// Sorted triggers by length (longest first) for matching
    sorted_triggers: Vec<String>,
}

impl ShortcutTable {
    pub fn new() -> Self {
        Self {
            shortcuts: HashMap::new(),
            sorted_triggers: vec![],
        }
    }

    /// Create with default Vietnamese shortcuts (common abbreviations for all methods)
    pub fn with_defaults() -> Self {
        let mut table = Self::new();

        // Common abbreviations (apply to all input methods)
        table.add(Shortcut::new("vn", "Việt Nam"));
        table.add(Shortcut::new("hcm", "Hồ Chí Minh"));
        table.add(Shortcut::new("hn", "Hà Nội"));
        table.add(Shortcut::new("dc", "được"));
        table.add(Shortcut::new("ko", "không"));

        // Telex-specific shortcuts
        // "w" → "ư" when not used as tone modifier (standalone w)
        table.add(Shortcut::telex("w", "ư"));

        table
    }

    /// Create with Telex defaults only
    pub fn with_telex_defaults() -> Self {
        let mut table = Self::new();

        // Telex-specific: "w" → "ư" (when standalone, not as modifier)
        table.add(Shortcut::telex("w", "ư"));

        table
    }

    /// Create with VNI defaults only
    pub fn with_vni_defaults() -> Self {
        // Currently no VNI-specific shortcuts
        Self::new()
    }

    /// Create with all defaults (common + method-specific)
    pub fn with_all_defaults() -> Self {
        let mut table = Self::new();

        // Common abbreviations (apply to all input methods)
        table.add(Shortcut::new("vn", "Việt Nam"));
        table.add(Shortcut::new("hcm", "Hồ Chí Minh"));
        table.add(Shortcut::new("hn", "Hà Nội"));
        table.add(Shortcut::new("dc", "được"));
        table.add(Shortcut::new("ko", "không"));

        // Telex-specific shortcuts
        table.add(Shortcut::telex("w", "ư"));

        table
    }

    /// Add a shortcut
    pub fn add(&mut self, shortcut: Shortcut) {
        let trigger = shortcut.trigger.clone();
        self.shortcuts.insert(trigger.clone(), shortcut);
        self.rebuild_sorted_triggers();
    }

    /// Remove a shortcut
    pub fn remove(&mut self, trigger: &str) -> Option<Shortcut> {
        let result = self.shortcuts.remove(&trigger.to_lowercase());
        if result.is_some() {
            self.rebuild_sorted_triggers();
        }
        result
    }

    /// Check if buffer matches any shortcut (for any input method)
    ///
    /// Returns (trigger, shortcut) if match found
    pub fn lookup(&self, buffer: &str) -> Option<(&str, &Shortcut)> {
        self.lookup_for_method(buffer, InputMethod::All)
    }

    /// Check if buffer matches any shortcut for specific input method
    ///
    /// Returns (trigger, shortcut) if match found
    pub fn lookup_for_method(
        &self,
        buffer: &str,
        method: InputMethod,
    ) -> Option<(&str, &Shortcut)> {
        let buffer_lower = buffer.to_lowercase();

        // Longest-match-first
        for trigger in &self.sorted_triggers {
            if buffer_lower == *trigger {
                if let Some(shortcut) = self.shortcuts.get(trigger) {
                    if shortcut.enabled && shortcut.applies_to(method) {
                        return Some((trigger, shortcut));
                    }
                }
            }
        }
        None
    }

    /// Try to match buffer with trigger key (for any input method)
    ///
    /// # Arguments
    /// * `buffer` - Current buffer content (as string)
    /// * `key_char` - The key that was just pressed
    /// * `is_word_boundary` - Whether key_char is a word boundary
    ///
    /// # Returns
    /// ShortcutMatch if a shortcut should be triggered
    pub fn try_match(
        &self,
        buffer: &str,
        key_char: Option<char>,
        is_word_boundary: bool,
    ) -> Option<ShortcutMatch> {
        self.try_match_for_method(buffer, key_char, is_word_boundary, InputMethod::All)
    }

    /// Try to match buffer with trigger key for specific input method
    ///
    /// # Arguments
    /// * `buffer` - Current buffer content (as string)
    /// * `key_char` - The key that was just pressed
    /// * `is_word_boundary` - Whether key_char is a word boundary
    /// * `method` - The current input method (Telex/VNI)
    ///
    /// # Returns
    /// ShortcutMatch if a shortcut should be triggered
    pub fn try_match_for_method(
        &self,
        buffer: &str,
        key_char: Option<char>,
        is_word_boundary: bool,
        method: InputMethod,
    ) -> Option<ShortcutMatch> {
        let (trigger, shortcut) = self.lookup_for_method(buffer, method)?;

        match shortcut.condition {
            TriggerCondition::Immediate => {
                let output = self.apply_case(buffer, &shortcut.replacement, shortcut.case_mode);
                Some(ShortcutMatch {
                    backspace_count: trigger.len(),
                    output,
                    include_trigger_key: false,
                })
            }
            TriggerCondition::OnWordBoundary => {
                if is_word_boundary {
                    let mut output =
                        self.apply_case(buffer, &shortcut.replacement, shortcut.case_mode);
                    // Append the trigger key (space, etc.)
                    if let Some(ch) = key_char {
                        output.push(ch);
                    }
                    Some(ShortcutMatch {
                        backspace_count: trigger.len(),
                        output,
                        include_trigger_key: true,
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Apply case transformation based on mode
    fn apply_case(&self, trigger: &str, replacement: &str, mode: CaseMode) -> String {
        match mode {
            CaseMode::Exact => replacement.to_string(),
            CaseMode::MatchCase => {
                if trigger.chars().all(|c| c.is_uppercase()) {
                    // All uppercase → replacement all uppercase
                    replacement.to_uppercase()
                } else if trigger
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                {
                    // First char uppercase → capitalize replacement
                    let mut chars = replacement.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => String::new(),
                    }
                } else {
                    // Lowercase → keep replacement as-is
                    replacement.to_string()
                }
            }
        }
    }

    /// Rebuild sorted triggers list (longest first)
    fn rebuild_sorted_triggers(&mut self) {
        self.sorted_triggers = self.shortcuts.keys().cloned().collect();
        self.sorted_triggers
            .sort_by_key(|s| std::cmp::Reverse(s.len()));
    }

    /// Check if shortcut table is empty
    pub fn is_empty(&self) -> bool {
        self.shortcuts.is_empty()
    }

    /// Get number of shortcuts
    pub fn len(&self) -> usize {
        self.shortcuts.len()
    }

    /// Clear all shortcuts
    pub fn clear(&mut self) {
        self.shortcuts.clear();
        self.sorted_triggers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_shortcut() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::new("vn", "Việt Nam"));

        let result = table.try_match("vn", Some(' '), true);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.backspace_count, 2);
        assert_eq!(m.output, "Việt Nam ");
    }

    #[test]
    fn test_case_matching() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::new("vn", "Việt Nam"));

        // Lowercase
        let m = table.try_match("vn", Some(' '), true).unwrap();
        assert_eq!(m.output, "Việt Nam ");

        // Uppercase
        let m = table.try_match("VN", Some(' '), true).unwrap();
        assert_eq!(m.output, "VIỆT NAM ");

        // Title case
        let m = table.try_match("Vn", Some(' '), true).unwrap();
        assert_eq!(m.output, "Việt Nam ");
    }

    #[test]
    fn test_immediate_shortcut() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::immediate("w", "ư"));

        // Immediate triggers without word boundary
        let result = table.try_match("w", None, false);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.output, "ư");
        assert!(!m.include_trigger_key);
    }

    #[test]
    fn test_word_boundary_required() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::new("vn", "Việt Nam"));

        // Without word boundary - should not match
        let result = table.try_match("vn", Some('a'), false);
        assert!(result.is_none());

        // With word boundary - should match
        let result = table.try_match("vn", Some(' '), true);
        assert!(result.is_some());
    }

    #[test]
    fn test_longest_match() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::new("h", "họ"));
        table.add(Shortcut::new("hcm", "Hồ Chí Minh"));

        // "hcm" should match the longer shortcut
        let (trigger, _) = table.lookup("hcm").unwrap();
        assert_eq!(trigger, "hcm");
    }

    #[test]
    fn test_disabled_shortcut() {
        let mut table = ShortcutTable::new();
        let mut shortcut = Shortcut::new("vn", "Việt Nam");
        shortcut.enabled = false;
        table.add(shortcut);

        let result = table.lookup("vn");
        assert!(result.is_none());
    }

    #[test]
    fn test_telex_specific_shortcut() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::telex("w", "ư"));

        // Should match for Telex
        let result = table.try_match_for_method("w", None, false, InputMethod::Telex);
        assert!(result.is_some());
        assert_eq!(result.unwrap().output, "ư");

        // Should NOT match for VNI
        let result = table.try_match_for_method("w", None, false, InputMethod::Vni);
        assert!(result.is_none());

        // Should match for All (fallback)
        let result = table.try_match_for_method("w", None, false, InputMethod::All);
        assert!(result.is_some());
    }

    #[test]
    fn test_vni_specific_shortcut() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::vni("7", "ơ"));

        // Should match for VNI
        let result = table.try_match_for_method("7", None, false, InputMethod::Vni);
        assert!(result.is_some());
        assert_eq!(result.unwrap().output, "ơ");

        // Should NOT match for Telex
        let result = table.try_match_for_method("7", None, false, InputMethod::Telex);
        assert!(result.is_none());
    }

    #[test]
    fn test_all_input_method_shortcut() {
        let mut table = ShortcutTable::new();
        table.add(Shortcut::new("vn", "Việt Nam"));

        // Should match for Telex
        let result = table.try_match_for_method("vn", Some(' '), true, InputMethod::Telex);
        assert!(result.is_some());

        // Should match for VNI
        let result = table.try_match_for_method("vn", Some(' '), true, InputMethod::Vni);
        assert!(result.is_some());

        // Should match for All
        let result = table.try_match_for_method("vn", Some(' '), true, InputMethod::All);
        assert!(result.is_some());
    }

    #[test]
    fn test_with_defaults_contains_telex_w() {
        let table = ShortcutTable::with_defaults();

        // "w" → "ư" should exist for Telex
        let result = table.lookup_for_method("w", InputMethod::Telex);
        assert!(result.is_some());
        let (_, shortcut) = result.unwrap();
        assert_eq!(shortcut.replacement, "ư");
        assert_eq!(shortcut.input_method, InputMethod::Telex);

        // "w" should NOT match for VNI
        let result = table.lookup_for_method("w", InputMethod::Vni);
        assert!(result.is_none());
    }

    #[test]
    fn test_shortcut_for_method_builder() {
        let shortcut = Shortcut::new("test", "Test").for_method(InputMethod::Telex);
        assert_eq!(shortcut.input_method, InputMethod::Telex);

        let shortcut = Shortcut::immediate("x", "y").for_method(InputMethod::Vni);
        assert_eq!(shortcut.input_method, InputMethod::Vni);
    }

    #[test]
    fn test_applies_to() {
        let all_shortcut = Shortcut::new("vn", "Việt Nam");
        assert!(all_shortcut.applies_to(InputMethod::All));
        assert!(all_shortcut.applies_to(InputMethod::Telex));
        assert!(all_shortcut.applies_to(InputMethod::Vni));

        let telex_shortcut = Shortcut::telex("w", "ư");
        assert!(telex_shortcut.applies_to(InputMethod::All));
        assert!(telex_shortcut.applies_to(InputMethod::Telex));
        assert!(!telex_shortcut.applies_to(InputMethod::Vni));

        let vni_shortcut = Shortcut::vni("7", "ơ");
        assert!(vni_shortcut.applies_to(InputMethod::All));
        assert!(!vni_shortcut.applies_to(InputMethod::Telex));
        assert!(vni_shortcut.applies_to(InputMethod::Vni));
    }
}
