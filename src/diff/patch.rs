use similar::TextDiff;

pub fn render_module_patch(
    baseline: Option<&str>,
    target: Option<&str>,
    header_baseline: &str,
    header_target: &str,
) -> Option<String> {
    let b = baseline.unwrap_or("");
    let t = target.unwrap_or("");
    if baseline.is_none() && target.is_none() {
        return None;
    }
    if b == t {
        return None;
    }
    let patch = TextDiff::from_lines(b, t)
        .unified_diff()
        .header(header_baseline, header_target)
        .to_string();
    Some(patch)
}
