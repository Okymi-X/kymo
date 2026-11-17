//! Files and texts that carry a participant's own flag. An author writes `{{flag}}` (the first
//! flag of the challenge) or `{{flag:name}}` (a named one) in a text file; each participant gets
//! the file with their generated value in place of the token. Pure: the values come in as a list.

/// Most characters of a template file. A bigger one is not a text a person wrote.
pub const MAX_TEMPLATE_BYTES: usize = 1 << 20;

/// A flag token found in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// `{{flag}}`: the challenge's first flag.
    First,
    /// `{{flag:name}}`.
    Named(String),
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Reads the token starting at the front of `rest` (which begins with `{{`), with its length.
fn token_at(rest: &str) -> Option<(Token, usize)> {
    let end = rest.find("}}")?;
    let inner = rest.get(2..end)?;
    let token = if inner == "flag" {
        Token::First
    } else {
        let name = inner.strip_prefix("flag:")?;
        if !valid_name(name) {
            return None;
        }
        Token::Named(name.to_owned())
    };
    Some((token, end + 2))
}

/// Every flag token in the text, in order of appearance.
pub fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = text[at..].find("{{") {
        let start = at + found;
        match token_at(&text[start..]) {
            Some((token, len)) => {
                out.push(token);
                at = start + len;
            }
            None => at = start + 1,
        }
    }
    out
}

/// The text with each token replaced by that flag's value. `values` lists `(name, value)` in the
/// challenge's flag order; `{{flag}}` is the first. A token with no value stays as written, so a
/// misspelt name shows up in the file instead of silently vanishing.
pub fn render(text: &str, values: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = text[at..].find("{{") {
        let start = at + found;
        out.push_str(&text[at..start]);
        match token_at(&text[start..]) {
            Some((token, len)) => {
                let value = match &token {
                    Token::First => values.first().map(|(_, v)| v),
                    Token::Named(name) => values.iter().find(|(n, _)| n == name).map(|(_, v)| v),
                };
                match value {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&text[start..start + len]),
                }
                at = start + len;
            }
            None => {
                out.push('{');
                at = start + 1;
            }
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Names a token refers to that the challenge does not have, for the author to fix.
pub fn unknown_names(text: &str, names: &[String]) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for token in tokens(text) {
        let bad = match &token {
            Token::First => names.is_empty().then(|| "flag".to_owned()),
            Token::Named(n) => (!names.contains(n)).then(|| n.clone()),
        };
        if let Some(name) = bad.filter(|n| !missing.contains(n)) {
            missing.push(name);
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Vec<(String, String)> {
        vec![
            ("user".into(), "KYMO{u_aaaa}".into()),
            ("root".into(), "KYMO{r_bbbb}".into()),
        ]
    }

    #[test]
    fn first_and_named_tokens_are_replaced_and_the_rest_is_untouched() {
        let text = "a {{flag}} b {{flag:root}} c {{flag:user}} {{other}} { {flag } }";
        assert_eq!(
            render(text, &values()),
            "a KYMO{u_aaaa} b KYMO{r_bbbb} c KYMO{u_aaaa} {{other}} { {flag } }"
        );
    }

    #[test]
    fn a_token_without_a_value_is_kept_as_written() {
        assert_eq!(
            render("{{flag:nope}} {{flag}}", &[]),
            "{{flag:nope}} {{flag}}"
        );
    }

    #[test]
    fn a_flag_value_that_looks_like_a_token_is_not_expanded_again() {
        let v = vec![("a".to_owned(), "{{flag:a}}".to_owned())];
        assert_eq!(render("{{flag:a}}", &v), "{{flag:a}}");
    }

    #[test]
    fn text_without_tokens_and_unusual_input_survive() {
        assert_eq!(render("", &values()), "");
        assert_eq!(render("plain", &values()), "plain");
        assert_eq!(render("{{", &values()), "{{");
        assert_eq!(render("{{flag", &values()), "{{flag");
        assert_eq!(
            render("ünï {{flag}} çödé", &values()),
            "ünï KYMO{u_aaaa} çödé"
        );
        assert_eq!(render("{{{flag}}}", &values()), "{KYMO{u_aaaa}}");
    }

    #[test]
    fn names_are_checked_for_shape() {
        assert_eq!(
            tokens("{{flag:a b}} {{flag:}} {{flag:ok-1_2}}"),
            vec![Token::Named("ok-1_2".into())]
        );
        assert_eq!(tokens("{{flag}}{{flag}}"), vec![Token::First, Token::First]);
    }

    #[test]
    fn unknown_names_are_listed_once() {
        let names = vec!["user".to_owned()];
        assert_eq!(
            unknown_names("{{flag:root}} {{flag:root}} {{flag:user}} {{flag}}", &names),
            vec!["root".to_owned()]
        );
        assert_eq!(unknown_names("{{flag}}", &[]), vec!["flag".to_owned()]);
    }
}
