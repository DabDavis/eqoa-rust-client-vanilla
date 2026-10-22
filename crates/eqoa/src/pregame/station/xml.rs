//! What `VIXMLParser` (vixmlparser.cpp) makes of a page: libxml2's tree of
//! the file (`xmlParseMemory`, which refuses a file that is not well formed:
//! no page then), walked depth first, an element's start and then its end
//! handed to the handler on top of the stack (`Walk`). Text is never read by
//! the page parser and comments are no elements, so only elements are kept.
//! The pages are 8-bit text; a byte is a character.

/// An element: its name, its attributes in order, its child elements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    /// `xmlGetProp`: the attribute, case-sensitively, entities resolved.
    pub fn prop(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// `VIXMLEvent::GetPropInt`: `atoi` of the attribute, else `def`.
    pub fn prop_int(&self, name: &str, def: i32) -> i32 {
        self.prop(name).map_or(def, atoi)
    }
}

/// `atoi`: leading white space, a sign, the digits; 0 for none.
pub fn atoi(s: &str) -> i32 {
    let s = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (neg, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for c in s.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + (c - b'0') as i64).min(i64::from(u32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v as i32
}

/// The document's root element; None for a file libxml would refuse.
pub fn parse(bytes: &[u8]) -> Option<Element> {
    let text: Vec<char> = bytes.iter().map(|&b| b as char).collect();
    let mut p = Parser { s: &text, i: 0 };
    p.misc()?;
    let root = p.element()?;
    p.misc()?;
    (p.i == text.len()).then_some(root)
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }

    fn starts(&self, t: &str) -> bool {
        t.chars().enumerate().all(|(k, c)| self.peek(k) == Some(c))
    }

    fn ws(&mut self) {
        while self.peek(0).is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
            self.i += 1;
        }
    }

    /// Up to and past `end`.
    fn skip_past(&mut self, end: &str) -> Option<()> {
        while self.i < self.s.len() {
            if self.starts(end) {
                self.i += end.chars().count();
                return Some(());
            }
            self.i += 1;
        }
        None
    }

    /// White space, comments, processing instructions and a doctype, where
    /// they may stand outside the root.
    fn misc(&mut self) -> Option<()> {
        loop {
            self.ws();
            if self.starts("<!--") {
                self.skip_past("-->")?;
            } else if self.starts("<?") {
                self.skip_past("?>")?;
            } else if self.starts("<!") {
                self.skip_past(">")?;
            } else {
                return Some(());
            }
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.i;
        while self.peek(0).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')) {
            self.i += 1;
        }
        (self.i > start && !self.s[start].is_ascii_digit()).then(|| self.s[start..self.i].iter().collect())
    }

    fn element(&mut self) -> Option<Element> {
        if self.peek(0) != Some('<') {
            return None;
        }
        self.i += 1;
        let name = self.name()?;
        let mut e = Element { name, ..Default::default() };
        loop {
            let had_space = self.peek(0).is_some_and(|c| c.is_whitespace());
            self.ws();
            match self.peek(0)? {
                '/' => {
                    if self.peek(1) != Some('>') {
                        return None;
                    }
                    self.i += 2;
                    return Some(e);
                }
                '>' => {
                    self.i += 1;
                    break;
                }
                _ => {
                    if !had_space {
                        return None;
                    }
                    let k = self.name()?;
                    self.ws();
                    if self.peek(0) != Some('=') {
                        return None;
                    }
                    self.i += 1;
                    self.ws();
                    let q = self.peek(0)?;
                    if q != '"' && q != '\'' {
                        return None;
                    }
                    self.i += 1;
                    let start = self.i;
                    while self.peek(0)? != q {
                        if self.peek(0) == Some('<') {
                            return None;
                        }
                        self.i += 1;
                    }
                    let raw: String = self.s[start..self.i].iter().collect();
                    self.i += 1;
                    if e.attrs.iter().any(|(n, _)| *n == k) {
                        return None;
                    }
                    e.attrs.push((k, unescape(&raw)?));
                }
            }
        }
        // content: elements, comments, text, then the end tag
        loop {
            if self.starts("</") {
                self.i += 2;
                let n = self.name()?;
                self.ws();
                if n != e.name || self.peek(0) != Some('>') {
                    return None;
                }
                self.i += 1;
                return Some(e);
            } else if self.starts("<!--") {
                self.skip_past("-->")?;
            } else if self.starts("<![CDATA[") {
                self.skip_past("]]>")?;
            } else if self.starts("<?") {
                self.skip_past("?>")?;
            } else if self.peek(0) == Some('<') {
                e.children.push(self.element()?);
            } else {
                // text: an entity in it must still be one
                let start = self.i;
                while self.peek(0).is_some_and(|c| c != '<') {
                    self.i += 1;
                }
                if self.i >= self.s.len() {
                    return None;
                }
                let t: String = self.s[start..self.i].iter().collect();
                unescape(&t)?;
            }
        }
    }
}

/// The five entities and character references.
fn unescape(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find(';')? + at;
        let ent = &rest[at + 1..end];
        let c = match ent {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let n = if let Some(h) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                    u32::from_str_radix(h, 16).ok()?
                } else {
                    ent.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(n)?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// `VIXMLParser::Walk`'s events: an element starts (0), its children are
/// walked, it ends (1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Start,
    End,
}

/// Every element's start and end in the order `Walk` hands them over.
pub fn walk<'a>(root: &'a Element, f: &mut dyn FnMut(Kind, &'a Element)) {
    f(Kind::Start, root);
    for c in &root.children {
        walk(c, f);
    }
    f(Kind::End, root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_parses_to_its_elements() {
        let src = b"<PS2GUIPage gridWidth=\"1\" gridHeight=\"7\">\r\n\t<!-- <Object><Position x=\"9\"/></Object> -->\r\n\t<Text name=\"A\" val=\"x &amp; y\">\r\n\t\t<Position x=\"65\" y='103'/>\r\n\t</Text>\r\n</PS2GUIPage>\r\n";
        let r = parse(src).unwrap();
        assert_eq!(r.name, "PS2GUIPage");
        assert_eq!(r.prop_int("gridHeight", 0), 7);
        assert_eq!(r.children.len(), 1, "the comment is no element");
        let t = &r.children[0];
        assert_eq!((t.prop("name"), t.prop("val")), (Some("A"), Some("x & y")));
        assert_eq!(t.children[0].prop_int("y", 0), 103);
        let mut ev = Vec::new();
        walk(&r, &mut |k, e| ev.push((k, e.name.clone())));
        assert_eq!(
            ev,
            vec![(Kind::Start, "PS2GUIPage".to_string()), (Kind::Start, "Text".into()), (Kind::Start, "Position".into()), (Kind::End, "Position".into()), (Kind::End, "Text".into()), (Kind::End, "PS2GUIPage".into())]
        );
    }

    #[test]
    fn what_libxml_refuses_is_no_page() {
        assert!(parse(b"<A><B></A>").is_none(), "mismatched end tag");
        assert!(parse(b"<A x=\"1\" x=\"2\"/>").is_none(), "an attribute twice");
        assert!(parse(b"<A x=1/>").is_none(), "an unquoted value");
        assert!(parse(b"<A/><B/>").is_none(), "two roots");
        assert!(parse(b"<A>&bogus;</A>").is_none(), "an unknown entity");
        // the control: the same, well formed
        assert!(parse(b"<?xml version=\"1.0\"?><A x=\"1\"><B/></A>").is_some());
    }

    #[test]
    fn atoi_is_cs() {
        assert_eq!([atoi(" 42x"), atoi("-5"), atoi("+7"), atoi("abc"), atoi("")], [42, -5, 7, 0, 0]);
    }
}
