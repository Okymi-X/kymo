use crate::error::ApiError;

/// Invisible or direction changing characters let a name look like another one.
pub fn is_hidden_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Escapes text for use inside HTML element content or a double quoted attribute.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Trims a display name and rejects empty, overlong or hidden-character input.
pub fn clean_name(raw: &str, max: usize) -> Result<String, ApiError> {
    if raw.chars().any(crate::text::is_hidden_char) {
        return Err(ApiError::BadRequest(
            "names cannot contain control or invisible characters".into(),
        ));
    }
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > max {
        return Err(ApiError::BadRequest(format!(
            "name must be 1 to {max} characters"
        )));
    }
    Ok(name.to_owned())
}

pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}
