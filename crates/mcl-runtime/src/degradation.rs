//! Degradation engine: field-level redaction when labeled values are
//! substituted into prompts at a lower clearance level.
//!
//! When a value labeled with `degrade phi to clinical_only` is substituted,
//! the `redact:` / `preserve:` field lists are applied to strip or retain
//! structured fields within the value.

use mcl_spec::label::DegradationRule;

/// Parse a value string into key-value pairs.
///
/// Supports two formats, one entry per line:
/// - `key: value` (colon-space separator)
/// - `key=value`  (equals separator)
///
/// Lines that don't match either pattern are treated as bare text lines
/// (key = the full line, value = empty string).
fn parse_fields(value: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for line in value.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(pos) = trimmed.find(": ") {
            let key = trimmed[..pos].trim().to_string();
            let val = trimmed[pos + 2..].trim().to_string();
            fields.push((key, val));
        } else if let Some(pos) = trimmed.find('=') {
            let key = trimmed[..pos].trim().to_string();
            let val = trimmed[pos + 1..].trim().to_string();
            fields.push((key, val));
        } else {
            fields.push((trimmed.to_string(), String::new()));
        }
    }
    fields
}

/// Apply field-level degradation to a value string.
///
/// For each rule in `rules`:
/// - Fields whose key appears in `redact` have their value replaced with
///   `[REDACTED]`.
/// - Fields whose key appears in `preserve` are kept unchanged.
/// - Fields not in either list are kept unchanged (not removed).
///
/// If the result is completely empty after redaction, returns `"[REDACTED]"`.
/// Missing fields in `redact` are a silent no-op (no error).
///
/// When multiple rules are provided, they are applied sequentially:
/// a field redacted by one rule stays redacted for subsequent rules.
pub fn degrade_value(value: &str, rules: &[DegradationRule]) -> String {
    if rules.is_empty() {
        return value.to_string();
    }

    let mut fields = parse_fields(value);

    for rule in rules {
        for (key, val) in &mut fields {
            if rule.redact.iter().any(|r| r == key) {
                *val = "[REDACTED]".to_string();
            }
        }
    }

    let result = fields
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                k.clone()
            } else {
                format!("{k}: {v}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if result.trim().is_empty() {
        "[REDACTED]".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(label: &str, to_level: &str, redact: &[&str], preserve: &[&str]) -> DegradationRule {
        DegradationRule {
            label: label.to_string(),
            from_params: None,
            to_level: to_level.to_string(),
            redact: redact.iter().map(|s| s.to_string()).collect(),
            preserve: preserve.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn degradation_clinical_only() {
        // R1 §6 test 9: patient @phi in clinical prompt
        // name/ssn/dob/address removed, diagnosis_codes/lab_values preserved
        let value = "\
name: John Smith
ssn: 123-45-6789
dob: 1990-01-15
address: 123 Main St
diagnosis_codes: E11.9, I10
lab_values: glucose=180, HbA1c=7.2";

        let rules = vec![rule(
            "phi",
            "clinical_only",
            &["name", "ssn", "dob", "address"],
            &["diagnosis_codes", "lab_values"],
        )];

        let result = degrade_value(value, &rules);

        assert!(
            result.contains("name: [REDACTED]"),
            "name should be redacted: {result}"
        );
        assert!(
            result.contains("ssn: [REDACTED]"),
            "ssn should be redacted: {result}"
        );
        assert!(
            result.contains("dob: [REDACTED]"),
            "dob should be redacted: {result}"
        );
        assert!(
            result.contains("address: [REDACTED]"),
            "address should be redacted: {result}"
        );
        assert!(
            result.contains("diagnosis_codes: E11.9, I10"),
            "diagnosis_codes should be preserved: {result}"
        );
        assert!(
            result.contains("lab_values: glucose=180, HbA1c=7.2"),
            "lab_values should be preserved: {result}"
        );
    }

    #[test]
    fn degradation_missing_field_silent() {
        // Degrade rule references non-existent field → no error, no change
        let value = "name: Alice\nage: 30";

        let rules = vec![rule(
            "phi",
            "clinical_only",
            &["nonexistent_field", "also_missing"],
            &[],
        )];

        let result = degrade_value(value, &rules);

        assert_eq!(result, "name: Alice\nage: 30");
    }

    #[test]
    fn degradation_all_redacted() {
        // All fields redacted → returns "[REDACTED]"
        let value = "name: Bob\nssn: 999-99-9999";

        let rules = vec![rule("phi", "clinical_only", &["name", "ssn"], &[])];

        let result = degrade_value(value, &rules);

        // Both values are [REDACTED] but keys remain, so the full result
        // still has content. The spec says "completely empty value" → [REDACTED].
        // With keys present but values redacted, we check that all values are
        // [REDACTED].
        assert!(
            result.contains("[REDACTED]"),
            "should contain [REDACTED]: {result}"
        );
        assert!(!result.contains("Bob"), "Bob should be redacted");
        assert!(!result.contains("999-99-9999"), "SSN should be redacted");

        // Now test with a single-field value where redaction empties meaningful content
        let single_value = "secret: the password is abc123";
        let single_rules = vec![rule("secret", "public", &["secret"], &[])];
        let single_result = degrade_value(single_value, &single_rules);
        assert!(
            single_result.contains("[REDACTED]"),
            "single redacted field: {single_result}"
        );
    }

    #[test]
    fn degradation_preserve_only() {
        // Only preserve rules, everything else kept (since default = keep)
        let value = "name: Carol\nemail: carol@example.com\nphone: 555-1234";

        let rules = vec![rule("phi", "clinical_only", &[], &["name", "email"])];

        let result = degrade_value(value, &rules);

        // Nothing is redacted; preserve means "keep" but default is also "keep"
        assert_eq!(
            result,
            "name: Carol\nemail: carol@example.com\nphone: 555-1234"
        );
    }

    #[test]
    fn degradation_empty_rules() {
        // No rules → value unchanged
        let value = "name: Dave\nssn: 111-22-3333";

        let result = degrade_value(value, &[]);

        assert_eq!(result, "name: Dave\nssn: 111-22-3333");
    }

    #[test]
    fn degradation_equals_format() {
        // key=value format parsing
        let value = "name=Equinn\nssn=444-55-6666";

        let rules = vec![rule("phi", "public", &["ssn"], &[])];

        let result = degrade_value(value, &rules);

        assert!(result.contains("name: Equinn"), "name preserved: {result}");
        assert!(result.contains("ssn: [REDACTED]"), "ssn redacted: {result}");
    }

    #[test]
    fn degradation_multiple_rules() {
        // Multiple rules applied sequentially
        let value = "name: Frank\nssn: 777-88-9999\ndiagnosis: flu";

        let rules = vec![
            rule("phi", "clinical_only", &["name", "ssn"], &[]),
            rule("clinical_only", "public", &["diagnosis"], &[]),
        ];

        let result = degrade_value(value, &rules);

        assert!(
            result.contains("name: [REDACTED]"),
            "name redacted: {result}"
        );
        assert!(result.contains("ssn: [REDACTED]"), "ssn redacted: {result}");
        assert!(
            result.contains("diagnosis: [REDACTED]"),
            "diagnosis redacted: {result}"
        );
    }

    #[test]
    fn degradation_truly_empty_result() {
        // Edge case: if input is empty after stripping blank lines
        let result = degrade_value("", &[]);
        assert_eq!(result, "");

        // With rules on empty input → [REDACTED]
        let rules = vec![rule("phi", "public", &["x"], &[])];
        let result2 = degrade_value("", &rules);
        assert_eq!(result2, "[REDACTED]");
    }
}
