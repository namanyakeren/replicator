use chrono::Local;

/// Waktu lokal untuk baris log.
pub fn now_local() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Bungkus nama identifier MySQL dengan backtick.
pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// Validasi nama schema/user agar aman dipakai sebagai identifier.
pub fn is_valid_ident(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Pencocokan wildcard sederhana (* dan ?), tidak peka huruf besar/kecil.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star: Option<usize> = None;
    let mut mark = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Potong teks pada batas karakter UTF-8.
pub fn truncate_chars(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if value.chars().count() <= max {
        return value.to_string();
    }
    let take = max.saturating_sub(3);
    let mut out: String = value.chars().take(take).collect();
    out.push_str("...");
    out
}

pub fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Tampilkan nilai rahasia tanpa membocorkan isinya.
pub fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        "(kosong)".to_string()
    } else {
        "********".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard() {
        assert!(glob_match("*", "apa_saja"));
        assert!(glob_match("log_*", "log_2024"));
        assert!(glob_match("user?", "users"));
        assert!(!glob_match("user?", "user"));
        assert!(glob_match("*.bak_?", "db.bak_1"));
    }

    #[test]
    fn ident() {
        assert!(is_valid_ident("_rep123"));
        assert!(!is_valid_ident("rep-1"));
        assert!(!is_valid_ident("rep`x"));
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate_chars("abcdef", 10), "abcdef");
        assert_eq!(truncate_chars("abcdef", 5), "ab...");
    }
}
