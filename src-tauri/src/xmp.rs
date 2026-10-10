use quick_xml::{Reader, events::Event};
use std::{fs::OpenOptions, io::Read, ops::Range, path::Path};

pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 200_000;
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const DC: &str = "http://purl.org/dc/elements/1.1/";

#[derive(Debug)]
pub(crate) enum Error {
    Unsafe(&'static str),
    Malformed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsafe(s) => write!(f, "XMP refused: {s}"),
            Self::Malformed(s) => write!(f, "Malformed XMP: {s}"),
        }
    }
}

fn malformed(e: impl std::fmt::Display) -> Error {
    Error::Malformed(e.to_string())
}

pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("XMP requires a regular file of at most 16 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("XMP exceeds 16 MiB".into());
    }
    Ok(bytes)
}

pub(crate) fn read(path: &Path) -> Result<String, String> {
    let content = String::from_utf8(read_bytes(path)?).map_err(|e| e.to_string())?;
    validate(&content).map_err(|e| e.to_string())?;
    Ok(content)
}

struct Attribute {
    name: String,
    value: String,
    range: Range<usize>,
}

struct Node {
    name: String,
    parent: Option<usize>,
    attributes: Vec<Attribute>,
    start: Range<usize>,
    end: Range<usize>,
    empty: bool,
    children: Vec<usize>,
}

struct Document<'a> {
    content: &'a str,
    nodes: Vec<Node>,
}

impl<'a> Document<'a> {
    fn parse(content: &'a str) -> Result<Self, Error> {
        if content.len() > MAX_BYTES {
            return Err(Error::Unsafe("size exceeds 16 MiB"));
        }
        // Refuse even incomplete declarations before any attempted repair.
        if content.contains("<!DOCTYPE") {
            return Err(Error::Unsafe("DOCTYPE is unsupported"));
        }
        if content.chars().any(|c| !xml_char(c)) {
            return Err(malformed("invalid XML character"));
        }
        let mut reader = Reader::from_str(content);
        reader.config_mut().check_comments = true;
        let mut nodes = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut roots = 0;
        let mut count = 0;
        loop {
            let offset = reader.buffer_position() as usize;
            let event = reader.read_event().map_err(malformed)?;
            let finish = reader.buffer_position() as usize;
            count += 1;
            if count > MAX_NODES {
                return Err(Error::Unsafe("node count exceeds 200000"));
            }
            match event {
                Event::Start(ref start) | Event::Empty(ref start) => {
                    if stack.len() >= MAX_DEPTH {
                        return Err(Error::Unsafe("depth exceeds 64"));
                    }
                    if stack.is_empty() {
                        roots += 1;
                        if roots != 1 {
                            return Err(malformed("multiple root elements"));
                        }
                    }
                    let mut attributes = Vec::new();
                    for attribute in start.attributes() {
                        let a = attribute.map_err(malformed)?;
                        count += 1;
                        if count > MAX_NODES {
                            return Err(Error::Unsafe("node count exceeds 200000"));
                        }
                        let name = std::str::from_utf8(a.key.as_ref()).map_err(malformed)?;
                        let raw = std::str::from_utf8(a.value.as_ref()).map_err(malformed)?;
                        if raw.contains('<') {
                            return Err(malformed("unescaped attribute markup"));
                        }
                        let value = quick_xml::escape::unescape(raw)
                            .map_err(malformed)?
                            .into_owned();
                        if value.chars().any(|c| !xml_char(c)) {
                            return Err(malformed("invalid attribute character reference"));
                        }
                        let mut from = a.key.as_ref().as_ptr() as usize - content.as_ptr() as usize;
                        while from > offset && content.as_bytes()[from - 1].is_ascii_whitespace() {
                            from -= 1;
                        }
                        let to = a.value.as_ref().as_ptr() as usize + a.value.len() + 1
                            - content.as_ptr() as usize;
                        attributes.push(Attribute {
                            name: name.into(),
                            value,
                            range: from..to,
                        });
                    }
                    let empty = matches!(event, Event::Empty(_));
                    nodes.push(Node {
                        name: std::str::from_utf8(start.name().as_ref())
                            .map_err(malformed)?
                            .into(),
                        parent: stack.last().copied(),
                        attributes,
                        start: offset..finish,
                        end: finish..finish,
                        empty,
                        children: Vec::new(),
                    });
                    let child = nodes.len() - 1;
                    if let Some(&parent) = stack.last() {
                        nodes[parent].children.push(child);
                    }
                    if !empty {
                        stack.push(nodes.len() - 1);
                    }
                }
                Event::End(_) => {
                    let index = stack.pop().ok_or_else(|| malformed("unexpected end tag"))?;
                    nodes[index].end = offset..finish;
                }
                Event::Text(text) => {
                    let text = text.decode().map_err(malformed)?;
                    if stack.is_empty() && !text.trim().trim_start_matches('\u{feff}').is_empty() {
                        return Err(malformed("text outside root"));
                    }
                    if text.contains("]]>") {
                        return Err(malformed("unescaped CDATA terminator"));
                    }
                }
                Event::GeneralRef(reference) => {
                    if stack.is_empty() {
                        return Err(malformed("reference outside root"));
                    }
                    let raw = reference.decode().map_err(malformed)?;
                    let decoded = quick_xml::escape::unescape(&format!("&{raw};"))
                        .map_err(malformed)?
                        .into_owned();
                    if decoded.chars().any(|c| !xml_char(c)) {
                        return Err(malformed("invalid character reference"));
                    }
                }
                Event::CData(_) if stack.is_empty() => return Err(malformed("CDATA outside root")),
                Event::DocType(_) => return Err(Error::Unsafe("DOCTYPE is unsupported")),
                Event::Eof => break,
                _ => {}
            }
        }
        if roots != 1 || !stack.is_empty() {
            return Err(malformed("missing or unclosed root"));
        }
        Ok(Self { content, nodes })
    }

    fn namespace(&self, mut index: usize, prefix: &str) -> Option<&str> {
        loop {
            for attribute in &self.nodes[index].attributes {
                if attribute.name.strip_prefix("xmlns:") == Some(prefix) {
                    return Some(&attribute.value);
                }
                if prefix.is_empty() && attribute.name == "xmlns" {
                    return Some(&attribute.value);
                }
            }
            index = self.nodes[index].parent?;
        }
    }

    fn matches(&self, index: usize, name: &str, uri: &str, local: &str) -> bool {
        let (prefix, actual) = name.split_once(':').unwrap_or(("", name));
        let canonical = match uri {
            RDF => "rdf",
            XMP => "xmp",
            DC => "dc",
            _ => "",
        };
        actual == local
            && self
                .namespace(index, prefix)
                .map_or(prefix == canonical, |ns| ns == uri)
    }

    fn descriptions(&self) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().enumerate().filter_map(|(i, n)| {
            (self.matches(i, &n.name, RDF, "Description")
                && n.parent
                    .is_none_or(|p| self.matches(p, &self.nodes[p].name, RDF, "RDF")))
            .then_some(i)
        })
    }

    fn about(&self, index: usize) -> Option<&str> {
        self.nodes[index]
            .attributes
            .iter()
            .find(|a| a.name.contains(':') && self.matches(index, &a.name, RDF, "about"))
            .map(|a| a.value.as_str())
    }

    fn image_descriptions(&self) -> impl Iterator<Item = usize> + '_ {
        let first = self.descriptions().next();
        let about = first.and_then(|i| self.about(i));
        self.descriptions().filter(move |&i| {
            Some(i) == first || about.is_some_and(|value| self.about(i) == Some(value))
        })
    }

    fn children<'b>(
        &'b self,
        parent: usize,
        uri: &'b str,
        local: &'b str,
    ) -> impl Iterator<Item = usize> + 'b {
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(move |&i| self.matches(i, &self.nodes[i].name, uri, local))
    }

    fn text(&self, index: usize) -> Option<String> {
        let n = &self.nodes[index];
        if n.empty {
            return Some(String::new());
        }
        let mut reader = Reader::from_str(&self.content[n.start.end..n.end.start]);
        let mut result = String::new();
        loop {
            match reader.read_event().ok()? {
                Event::Text(t) => result.push_str(&t.decode().ok()?),
                Event::CData(t) => result.push_str(&t.decode().ok()?),
                Event::GeneralRef(r) => result.push_str(
                    &quick_xml::escape::unescape(&format!("&{};", r.decode().ok()?)).ok()?,
                ),
                Event::Comment(_) | Event::PI(_) => {}
                Event::Eof => return Some(result),
                _ => return None,
            }
        }
    }

    fn scalar(&self, local: &str) -> Option<String> {
        for i in self.image_descriptions() {
            for a in &self.nodes[i].attributes {
                if a.name.contains(':') && self.matches(i, &a.name, XMP, local) {
                    return Some(a.value.clone());
                }
            }
            if let Some(child) = self.children(i, XMP, local).next() {
                return self.text(child);
            }
        }
        None
    }

    fn prefix(&self, index: usize, preferred: &str, uri: &str) -> (String, String) {
        for suffix in 0..MAX_NODES {
            let prefix = if suffix == 0 {
                preferred.into()
            } else {
                format!("rr{preferred}{suffix}")
            };
            match self.namespace(index, &prefix) {
                Some(ns) if ns == uri => return (prefix, String::new()),
                None => return (prefix.clone(), format!(" xmlns:{prefix}=\"{uri}\"")),
                _ => {}
            }
        }
        unreachable!("bounded namespace declarations")
    }
}

fn xml_char(c: char) -> bool {
    matches!(c as u32, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

pub(crate) fn validate(content: &str) -> Result<(), Error> {
    Document::parse(content).map(|_| ())
}

pub(crate) fn element_text(content: &str, uri: &str, local: &str) -> Result<Option<String>, Error> {
    let doc = Document::parse(content)?;
    let Some(index) = doc.nodes.iter().enumerate().find_map(|(i, node)| {
        let (prefix, actual) = node.name.split_once(':').unwrap_or(("", &node.name));
        (actual == local && doc.namespace(i, prefix) == Some(uri)).then_some(i)
    }) else {
        return Ok(None);
    };
    if let Some(text) = doc.text(index) {
        return Ok(Some(text));
    }
    for (i, node) in doc.nodes.iter().enumerate() {
        let mut parent = node.parent;
        while let Some(p) = parent {
            if p == index {
                if let Some(text) = doc.text(i).filter(|t| !t.trim().is_empty()) {
                    return Ok(Some(text));
                }
                break;
            }
            parent = doc.nodes[p].parent;
        }
    }
    Ok(None)
}

#[derive(Default)]
pub(crate) struct Metadata {
    pub rating: Option<i8>,
    pub label: Option<String>,
    pub tags: Vec<String>,
}

pub(crate) fn metadata(content: &str) -> Metadata {
    let Ok(doc) = Document::parse(content) else {
        return Metadata::default();
    };
    let mut tags = Vec::new();
    for description in doc.image_descriptions() {
        for subject in doc.children(description, DC, "subject") {
            for bag in doc.children(subject, RDF, "Bag") {
                for li in doc.children(bag, RDF, "li") {
                    if let Some(value) = doc.text(li) {
                        tags.push(value);
                    }
                }
            }
        }
    }
    Metadata {
        rating: doc.scalar("Rating").and_then(|s| s.trim().parse().ok()),
        label: doc.scalar("Label"),
        tags,
    }
}

pub(crate) const SKELETON: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\"/></rdf:RDF></x:xmpmeta>";

pub(crate) fn update(
    content: &str,
    rating: i16,
    label: Option<&str>,
    tags: &[String],
) -> Result<String, Error> {
    if label
        .into_iter()
        .chain(tags.iter().map(String::as_str))
        .any(|s| s.chars().any(|c| !xml_char(c)))
    {
        return Err(malformed("metadata contains invalid XML characters"));
    }
    let doc = Document::parse(content)?;
    let index = doc
        .descriptions()
        .next()
        .ok_or_else(|| malformed("no outer RDF description"))?;
    let n = &doc.nodes[index];
    let (xmp, xmp_ns) = doc.prefix(index, "xmp", XMP);
    let (dc, dc_ns) = doc.prefix(index, "dc", DC);
    let (rdf, rdf_ns) = doc.prefix(index, "rdf", RDF);
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for description in doc.image_descriptions() {
        for a in &doc.nodes[description].attributes {
            if a.name.contains(':')
                && ["Rating", "Label"]
                    .iter()
                    .any(|local| doc.matches(description, &a.name, XMP, local))
            {
                edits.push((a.range.clone(), String::new()));
            }
        }
        for (uri, local) in [(XMP, "Rating"), (XMP, "Label"), (DC, "subject")] {
            for i in doc.children(description, uri, local) {
                let node = &doc.nodes[i];
                edits.push((node.start.start..node.end.end, String::new()));
            }
        }
    }
    let mut attributes = xmp_ns;
    let mut subject = String::new();
    if !tags.is_empty() {
        attributes.push_str(&dc_ns);
        attributes.push_str(&rdf_ns);
        subject.push_str(&format!("<{dc}:subject><{rdf}:Bag>"));
        for tag in tags {
            subject.push_str(&format!(
                "<{rdf}:li>{}</{rdf}:li>",
                quick_xml::escape::escape(tag)
            ));
        }
        subject.push_str(&format!("</{rdf}:Bag></{dc}:subject>"));
    }
    attributes.push_str(&format!(" {xmp}:Rating=\"{rating}\""));
    if let Some(label) = label {
        attributes.push_str(&format!(
            " {xmp}:Label=\"{}\"",
            quick_xml::escape::escape(label)
        ));
    }
    let insert = n.start.end - if n.empty { 2 } else { 1 };
    if n.empty && !subject.is_empty() {
        edits.push((
            insert..n.start.end,
            format!("{attributes}>{subject}</{}>", n.name),
        ));
    } else {
        edits.push((insert..insert, attributes));
        if !subject.is_empty() {
            edits.push((n.end.start..n.end.start, subject));
        }
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut result = content.to_string();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    validate(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_special_keywords_labels_and_numeric_entities() {
        let tags = vec![
            "R&D <family> \"東京\" '♥' $1".into(),
            "amp &amp; literal".into(),
        ];
        let result = update(SKELETON, 4, Some("Red & <gold>"), &tags).unwrap();
        let actual = metadata(&result);
        assert_eq!(actual.rating, Some(4));
        assert_eq!(actual.label.as_deref(), Some("Red & <gold>"));
        assert_eq!(actual.tags, tags);
        let escaped = result.replace("$1", "&#36;&#x31;");
        assert_eq!(metadata(&escaped).tags, tags);
        assert_eq!(
            update(&result, 4, Some("Red & <gold>"), &tags).unwrap(),
            result
        );
    }

    #[test]
    fn replaces_and_removes_metadata_across_same_subject_descriptions() {
        let nested = "<f:item><rdf:Description rdf:about='' xmp:Label='nested'/></f:item>";
        let foreign = "<rdf:Description rdf:about='other' xmp:Rating='1' xmp:Label='foreign'><dc:subject><rdf:Bag><rdf:li>foreign tag</rdf:li></rdf:Bag></dc:subject></rdf:Description>";
        let source = format!(
            "<rdf:RDF xmlns:rdf='{RDF}' xmlns:xmp='{XMP}' xmlns:dc='{DC}' xmlns:f='urn:foreign'><rdf:Description rdf:about='' xmp:Rating='2'/><rdf:Description rdf:about='' xmp:Label='Red' f:keep='yes'><dc:subject><rdf:Bag><rdf:li>old</rdf:li></rdf:Bag></dc:subject>{nested}</rdf:Description><rdf:Description rdf:about=''><xmp:Rating>3</xmp:Rating><xmp:Label>Blue</xmp:Label><dc:subject><rdf:Bag><rdf:li>second</rdf:li></rdf:Bag></dc:subject></rdf:Description>{foreign}</rdf:RDF>"
        );
        assert_eq!(metadata(&source).tags, ["old", "second"]);
        for label in [Some("Gold & green"), None] {
            for tags in [vec!["new & tag".into()], Vec::new()] {
                let updated = update(&source, 5, label, &tags).unwrap();
                let actual = metadata(&updated);
                assert_eq!(actual.rating, Some(5));
                assert_eq!(actual.label.as_deref(), label);
                assert_eq!(actual.tags, tags);
                assert!(updated.contains(nested));
                assert!(updated.contains(foreign));
                assert!(updated.contains("f:keep='yes'"));
                assert_eq!(update(&updated, 5, label, &tags).unwrap(), updated);
            }
        }
    }

    #[test]
    fn scopes_image_metadata_by_decoded_rdf_subject() {
        let source = format!(
            "<r:RDF xmlns:r='{RDF}' xmlns:a='{XMP}' xmlns:d='{DC}'><r:Description r:about='photo&amp;one'/><r:Description r:about='photo&#38;one' a:Label='Red'><d:subject><r:Bag><r:li>old</r:li></r:Bag></d:subject></r:Description><r:Description r:about='other' a:Label='other'/></r:RDF>"
        );
        assert_eq!(metadata(&source).label.as_deref(), Some("Red"));
        let result = update(&source, 4, None, &[]).unwrap();
        assert_eq!(metadata(&result).rating, Some(4));
        assert_eq!(metadata(&result).label, None);
        assert!(metadata(&result).tags.is_empty());
        assert!(result.contains("r:about='other' a:Label='other'"));

        let anonymous = format!(
            "<r:RDF xmlns:r='{RDF}' xmlns:a='{XMP}'><r:Description a:Label='first'/><r:Description a:Label='second'/></r:RDF>"
        );
        let result = update(&anonymous, 3, None, &[]).unwrap();
        assert_eq!(metadata(&result).label, None);
        assert!(result.contains("a:Label='second'"));
    }

    #[test]
    fn preserves_foreign_and_nested_bytes_with_namespace_collisions() {
        let foreign = "<f:item id='opaque&amp;id'><![CDATA[<&>]]><rdf:Description xmlns:xmp='urn:foreign' xmp:Rating='99'><xmp:Label>keep</xmp:Label></rdf:Description></f:item>";
        let source = format!(
            "<r:RDF xmlns:r='{RDF}' xmlns:rdf='{RDF}' xmlns:xmp='urn:foreign' xmlns:f='urn:foreign'><r:Description f:attr='a&amp;b' xmp:Rating='88'>{foreign}<a:Rating xmlns:a='{XMP}'>2</a:Rating><d:subject xmlns:d='{DC}'><r:Bag><r:li>old &amp; tag</r:li></r:Bag></d:subject></r:Description></r:RDF>"
        );
        assert_eq!(metadata(&source).rating, Some(2));
        assert_eq!(metadata(&source).tags, ["old & tag"]);
        let result = update(&source, 5, None, &["new & tag".into()]).unwrap();
        assert!(result.contains(foreign));
        assert!(result.contains("f:attr='a&amp;b' xmp:Rating='88'"));
        assert_eq!(metadata(&result).rating, Some(5));
        assert_eq!(metadata(&result).tags, ["new & tag"]);
        assert!(!result.contains("<a:Rating"));
    }

    #[test]
    fn updates_self_closing_descriptions_with_and_without_children() {
        let source = format!("<rdf:RDF xmlns:rdf='{RDF}'><rdf:Description keep='yes'/></rdf:RDF>");
        for tags in [Vec::new(), vec!["one".into()]] {
            let updated = update(&source, -1, Some("Blue"), &tags).unwrap();
            assert!(updated.contains("keep='yes'"));
            assert!(updated.contains(&format!("xmlns:xmp=\"{XMP}\"")));
            assert_eq!(metadata(&updated).rating, Some(-1));
            assert_eq!(metadata(&updated).tags, tags);
        }
    }

    #[test]
    fn refuses_unsafe_or_malformed_documents() {
        for content in [
            "<!DOCTYPE root [<!ENTITY x SYSTEM 'file:///secret'>]><root>&x;</root>",
            "<!DOCTYPE",
        ] {
            assert!(matches!(validate(content), Err(Error::Unsafe(_))));
        }
        for content in [
            "<a>",
            "<a></b>",
            "<a/><b/>",
            "<a>&custom;</a>",
            "<a x='&custom;'/>",
            "<a>&#0;</a>",
            "<a x='&#0;'/>",
            "<a x='<bad>'/>",
            "<a><!--a--b--></a>",
            "<a>]]></a>",
        ] {
            assert!(
                matches!(validate(content), Err(Error::Malformed(_))),
                "{content}"
            );
        }
        assert!(validate(&format!("{}{}", "<a>".repeat(64), "</a>".repeat(64))).is_ok());
        assert!(matches!(
            validate(&format!("{}{}", "<a>".repeat(65), "</a>".repeat(65))),
            Err(Error::Unsafe(_))
        ));
        assert!(matches!(
            validate(&format!("<a>{}</a>", "<b/>".repeat(200_000))),
            Err(Error::Unsafe(_))
        ));
        assert!(matches!(
            validate(&" ".repeat(MAX_BYTES + 1)),
            Err(Error::Unsafe(_))
        ));
    }

    #[test]
    fn bounded_regular_file_reads_reject_sparse_oversize_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_bytes(dir.path()).is_err());
        let path = dir.path().join("large.xmp");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(read_bytes(&path).is_err());
        std::fs::write(&path, SKELETON).unwrap();
        assert_eq!(read(&path).unwrap(), SKELETON);
    }
}
