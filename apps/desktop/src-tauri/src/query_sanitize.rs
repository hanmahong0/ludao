//! 用户可见检索字符串清洗：禁通配注入、限长、FTS 安全分词。

pub const MAX_QUERY_CHARS: usize = 128;
pub const MAX_PUBLIC_LIMIT: i64 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryReject {
    Empty,
    TooLong,
    WildcardForbidden,
}

impl std::fmt::Display for QueryReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryReject::Empty => write!(f, "query_rejected: empty"),
            QueryReject::TooLong => write!(f, "query_rejected: too_long"),
            QueryReject::WildcardForbidden => write!(f, "query_rejected: wildcard_forbidden"),
        }
    }
}

fn is_term_char(c: char) -> bool {
    c.is_alphanumeric() || c.is_whitespace() || ('\u{4e00}'..='\u{9fff}').contains(&c)
}

/// 检索词面：先拒绝 `%`/`_`，再丢掉引号和其它标点，使 `"借贷"` 与 `借贷` 同一词面。
pub fn literal_terms(q: &str) -> Result<String, QueryReject> {
    let t = q.trim();
    if t.is_empty() {
        return Err(QueryReject::Empty);
    }
    if t.chars().count() > MAX_QUERY_CHARS {
        return Err(QueryReject::TooLong);
    }
    if t.contains('%') || t.contains('_') {
        return Err(QueryReject::WildcardForbidden);
    }
    let cleaned: String = t.chars().filter(|c| is_term_char(*c)).collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return Err(QueryReject::Empty);
    }
    Ok(collapsed)
}

pub fn normalize_private_query(q: &str) -> Result<String, QueryReject> {
    literal_terms(q)
}

pub fn escape_like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 4);
    for c in q.chars() {
        match c {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// FTS5 查询：仅保留字母数字与中日韩统一表意文字，按空格 AND。
pub fn fts_and_query(q: &str) -> Result<String, QueryReject> {
    let t = literal_terms(q)?;
    let mut terms = Vec::new();
    for w in t.split_whitespace() {
        let clean: String = w
            .chars()
            .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c))
            .collect();
        if !clean.is_empty() {
            terms.push(format!("\"{clean}\""));
        }
    }
    if terms.is_empty() {
        return Err(QueryReject::Empty);
    }
    Ok(terms.join(" AND "))
}

pub fn clamp_limit(limit: i64, max: i64) -> i64 {
    limit.clamp(1, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_percent() {
        assert_eq!(
            normalize_private_query("%"),
            Err(QueryReject::WildcardForbidden)
        );
    }

    #[test]
    fn fts_chinese_and_multiword() {
        let q = fts_and_query("借贷 合同").unwrap();
        assert!(q.contains("借贷"));
        assert!(q.contains("AND"));
        let one = fts_and_query("贷").unwrap();
        assert!(one.contains("贷"));
    }

    #[test]
    fn strips_punctuation_for_fts() {
        let q = fts_and_query("\"借条（合成）\"").unwrap();
        assert!(!q.contains('%'));
        assert!(q.contains("借条"));
    }

    #[test]
    fn quoted_chinese_keeps_term() {
        assert_eq!(literal_terms("\"借贷\"").unwrap(), "借贷");
        assert_eq!(literal_terms("“借贷”").unwrap(), "借贷");
    }

    #[test]
    fn empty_and_length_limits() {
        assert_eq!(normalize_private_query("   "), Err(QueryReject::Empty));
        let long = "x".repeat(MAX_QUERY_CHARS + 1);
        assert_eq!(normalize_private_query(&long), Err(QueryReject::TooLong));
    }
}