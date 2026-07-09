#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingStyle {
    Lf,
    Crlf,
    Mixed,
}

pub fn detect_line_ending_style(text: &str) -> LineEndingStyle {
    let mut has_crlf = false;
    let mut has_lf = false;
    let mut has_lone_cr = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\r' {
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                has_crlf = true;
                i += 2;
                continue;
            } else {
                has_lone_cr = true;
            }
        } else if ch == '\n' {
            has_lf = true;
        }
        i += 1;
    }
    if has_lone_cr || (has_crlf && has_lf) {
        LineEndingStyle::Mixed
    } else if has_crlf {
        LineEndingStyle::Crlf
    } else {
        LineEndingStyle::Lf
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTextView {
    pub text: String,
    pub line_ending_style: LineEndingStyle,
}

pub fn to_model_text_view(raw: &str) -> ModelTextView {
    let style = detect_line_ending_style(raw);
    let text = if style == LineEndingStyle::Crlf {
        raw.replace("\r\n", "\n")
    } else {
        raw.to_string()
    };
    ModelTextView {
        text,
        line_ending_style: style,
    }
}

pub fn make_carriage_returns_visible(text: &str) -> String {
    text.replace('\r', "\\r")
}

pub fn materialize_model_text(text: &str, line_ending_style: LineEndingStyle) -> String {
    if line_ending_style != LineEndingStyle::Crlf {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lf() {
        assert_eq!(detect_line_ending_style("a\nb"), LineEndingStyle::Lf);
    }

    #[test]
    fn detects_crlf() {
        assert_eq!(detect_line_ending_style("a\r\nb"), LineEndingStyle::Crlf);
    }

    #[test]
    fn detects_mixed_when_lone_cr_present() {
        assert_eq!(detect_line_ending_style("a\rb"), LineEndingStyle::Mixed);
    }

    #[test]
    fn detects_mixed_when_crlf_and_lf_mixed() {
        assert_eq!(
            detect_line_ending_style("a\r\nb\nc"),
            LineEndingStyle::Mixed
        );
    }

    #[test]
    fn to_model_view_normalizes_crlf() {
        let v = to_model_text_view("a\r\nb");
        assert_eq!(v.text, "a\nb");
        assert_eq!(v.line_ending_style, LineEndingStyle::Crlf);
    }

    #[test]
    fn makes_lone_cr_visible() {
        assert_eq!(make_carriage_returns_visible("a\rb"), "a\\rb");
    }

    #[test]
    fn materialize_leaves_lf_unchanged() {
        assert_eq!(materialize_model_text("a\nb", LineEndingStyle::Lf), "a\nb");
    }

    #[test]
    fn materialize_leaves_mixed_unchanged() {
        assert_eq!(
            materialize_model_text("a\r\nb\nc", LineEndingStyle::Mixed),
            "a\r\nb\nc"
        );
    }

    #[test]
    fn materialize_converts_lf_to_crlf() {
        assert_eq!(
            materialize_model_text("a\nb", LineEndingStyle::Crlf),
            "a\r\nb"
        );
    }

    #[test]
    fn materialize_normalizes_existing_crlf_before_expanding() {
        assert_eq!(
            materialize_model_text("a\r\nb", LineEndingStyle::Crlf),
            "a\r\nb"
        );
    }
}
