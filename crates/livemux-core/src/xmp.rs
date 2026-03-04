use crate::constants;
use crate::error::{LiveMuxError, Result};

/// XMP document built from a string template with targeted replacements.
///
/// `xmltree` cannot preserve namespace-prefixed attributes (e.g. `GCamera:MotionPhoto`),
/// so we operate directly on the raw XML string. This works because the XMP template
/// has a fixed, known structure and all modifications are value replacements at known positions.
pub struct XmpDocument {
    xml: String,
}

impl XmpDocument {
    pub fn from_template() -> Result<Self> {
        Ok(Self {
            xml: constants::XMP_TEMPLATE.to_string(),
        })
    }

    /// Set GCamera:MotionPhotoPresentationTimestampUs value.
    pub fn set_timestamp(&mut self, microseconds: i64) -> Result<()> {
        self.xml = self.xml.replace(
            "GCamera:MotionPhotoPresentationTimestampUs=\"-1\"",
            &format!(
                "GCamera:MotionPhotoPresentationTimestampUs=\"{}\"",
                microseconds
            ),
        );
        Ok(())
    }

    /// Set Item:Mime on Primary item (for JPG images).
    pub fn set_primary_mime(&mut self, mime: &str) -> Result<()> {
        // The Primary item line in the template
        self.replace_in_item("Primary", "Item:Mime", mime)
    }

    /// Set Item:Mime on MotionPhoto item (for MP4 videos).
    pub fn set_motionphoto_mime(&mut self, mime: &str) -> Result<()> {
        self.replace_in_item("MotionPhoto", "Item:Mime", mime)
    }

    /// Set Item:Length on MotionPhoto item (video size in bytes).
    pub fn set_motionphoto_length(&mut self, length: usize) -> Result<()> {
        self.replace_in_item("MotionPhoto", "Item:Length", &length.to_string())
    }

    /// Set Item:Padding on Primary item (image padding).
    pub fn set_primary_padding(&mut self, padding: usize) -> Result<()> {
        self.replace_in_item("Primary", "Item:Padding", &padding.to_string())
    }

    /// Merge source XMP from the original image into our template.
    /// Copies all child elements and attributes from the source's rdf:Description,
    /// except Container:Directory (to avoid duplication).
    pub fn merge_source_xmp(&mut self, source_xmp: &str) -> Result<()> {
        // Extract attributes from source rdf:Description
        let desc_start = source_xmp.find("<rdf:Description").ok_or_else(|| {
            LiveMuxError::XmpElementMissing("rdf:Description in source".into())
        })?;

        // Find where the Description tag's attributes end and children begin
        let after_desc = &source_xmp[desc_start..];

        // Extract attributes from the opening rdf:Description tag
        // They are between <rdf:Description ... > (before the first > or />)
        if let Some(attrs_end) = find_description_attrs_end(after_desc) {
            let desc_tag = &after_desc[..attrs_end];

            // Extract xmlns:* declarations not already in template
            let xmlns_attrs = extract_xmlns_attrs(desc_tag);
            if !xmlns_attrs.is_empty() {
                // Insert xmlns declarations into template's rdf:Description
                let insert_point = "xmlns:HDRGainMap=";
                if let Some(pos) = self.xml.find(insert_point) {
                    // Find end of this xmlns attribute value
                    let after = &self.xml[pos..];
                    if let Some(q1) = after.find('"') {
                        if let Some(q2) = after[q1 + 1..].find('"') {
                            let insert_at = pos + q1 + 1 + q2 + 1;
                            let ns_str: String = xmlns_attrs
                                .iter()
                                .filter(|a| !self.xml.contains(a.as_str()))
                                .map(|a| format!("\n        {}", a))
                                .collect();
                            if !ns_str.is_empty() {
                                self.xml.insert_str(insert_at, &ns_str);
                            }
                        }
                    }
                }
            }

            // Extract non-xmlns attributes (skip xmlns:* and rdf:about)
            let attrs = extract_description_attrs(desc_tag);
            if !attrs.is_empty() {
                // Insert attributes into our template's rdf:Description
                let insert_point = "GCamera:MotionPhoto=\"1\"";
                if let Some(pos) = self.xml.find(insert_point) {
                    let insert_at = pos + insert_point.len();
                    let attr_str: String =
                        attrs.iter().map(|a| format!("\n      {}", a)).collect();
                    self.xml.insert_str(insert_at, &attr_str);
                }
            }
        }

        // Extract child elements from source (everything between > and </rdf:Description>)
        if let Some(children_xml) = extract_description_children(source_xmp) {
            // Filter out Container:Directory
            let filtered = filter_container_directory(&children_xml);
            if !filtered.trim().is_empty() {
                // Insert before </rdf:Description> in our template
                let close_tag = "</rdf:Description>";
                if let Some(pos) = self.xml.find(close_tag) {
                    self.xml.insert_str(pos, &format!("      {}\n    ", filtered.trim()));
                }
            }
        }

        Ok(())
    }

    /// Serialize the XMP document to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.xml.as_bytes().to_vec()
    }
}

/// Replace an attribute value within a Container:Item block identified by Item:Semantic.
impl XmpDocument {
    fn replace_in_item(&mut self, semantic: &str, attr: &str, new_value: &str) -> Result<()> {
        // Find the Container:Item line containing this semantic
        let semantic_marker = &format!("Item:Semantic=\"{}\"", semantic);
        let item_pos = self.xml.find(semantic_marker).ok_or_else(|| {
            LiveMuxError::XmpElementMissing(format!("Item with Semantic={}", semantic))
        })?;

        // Find the Container:Item element boundaries around this position
        let before = &self.xml[..item_pos];
        let item_start = before.rfind("<Container:Item").ok_or_else(|| {
            LiveMuxError::XmpElementMissing("Container:Item tag".into())
        })?;
        let after_item = &self.xml[item_start..];
        let item_end = after_item
            .find("/>")
            .map(|p| item_start + p + 2)
            .or_else(|| after_item.find('>').map(|p| item_start + p + 1))
            .ok_or_else(|| LiveMuxError::XmpElementMissing("Container:Item end".into()))?;

        let item_str = &self.xml[item_start..item_end];

        // Find and replace the attribute within this item
        let attr_prefix = &format!("{}=\"", attr);
        if let Some(attr_pos) = item_str.find(attr_prefix) {
            let val_start = attr_pos + attr_prefix.len();
            if let Some(val_end) = item_str[val_start..].find('"') {
                let abs_start = item_start + val_start;
                let abs_end = item_start + val_start + val_end;
                self.xml.replace_range(abs_start..abs_end, new_value);
                return Ok(());
            }
        }

        Err(LiveMuxError::XmpElementMissing(format!(
            "Attribute {} in Item Semantic={}",
            attr, semantic
        )))
    }
}

/// Find where the rdf:Description opening tag ends (the first > after attributes).
fn find_description_attrs_end(tag_str: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in tag_str.char_indices() {
        match ch {
            '"' => depth = 1 - depth, // toggle inside/outside quotes
            '>' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract xmlns:* namespace declarations from an rdf:Description tag string.
fn extract_xmlns_attrs(tag: &str) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut remaining = tag;

    loop {
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };

        let before_eq = remaining[..eq_pos].trim_end();
        let name_start = before_eq
            .rfind(|c: char| c.is_whitespace() || c == '<')
            .map(|p| p + 1)
            .unwrap_or(0);
        let attr_name = &before_eq[name_start..];

        let after_eq = &remaining[eq_pos + 1..];
        let quote_char = after_eq.chars().find(|&c| c == '"' || c == '\'');
        if let Some(q) = quote_char {
            let val_start = after_eq.find(q).unwrap() + 1;
            if let Some(val_end) = after_eq[val_start..].find(q) {
                let value = &after_eq[val_start..val_start + val_end];

                if attr_name.starts_with("xmlns:") {
                    attrs.push(format!("{}=\"{}\"", attr_name, value));
                }

                remaining = &after_eq[val_start + val_end + 1..];
                continue;
            }
        }
        break;
    }
    attrs
}

/// Extract meaningful attributes from rdf:Description tag string.
/// Skips xmlns:*, rdf:about, and rdf:parseType.
fn extract_description_attrs(tag: &str) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut remaining = tag;

    loop {
        // Find next attribute pattern: name="value"
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };

        // Get attribute name (word before =)
        let before_eq = remaining[..eq_pos].trim_end();
        let name_start = before_eq
            .rfind(|c: char| c.is_whitespace() || c == '<')
            .map(|p| p + 1)
            .unwrap_or(0);
        let attr_name = &before_eq[name_start..];

        // Get attribute value (between quotes after =)
        let after_eq = &remaining[eq_pos + 1..];
        let quote_char = after_eq.chars().find(|&c| c == '"' || c == '\'');
        if let Some(q) = quote_char {
            let val_start = after_eq.find(q).unwrap() + 1;
            if let Some(val_end) = after_eq[val_start..].find(q) {
                let value = &after_eq[val_start..val_start + val_end];
                let full_attr = format!("{}=\"{}\"", attr_name, value);

                // Skip namespace declarations, rdf:about, and empty attributes
                if !attr_name.starts_with("xmlns:")
                    && attr_name != "rdf:about"
                    && attr_name != "rdf:parseType"
                    && !attr_name.is_empty()
                    && !attr_name.contains('<')
                    // Skip GCamera attrs that are already in template
                    && !attr_name.starts_with("GCamera:")
                {
                    attrs.push(full_attr);
                }

                remaining = &after_eq[val_start + val_end + 1..];
                continue;
            }
        }
        break;
    }
    attrs
}

/// Extract child elements from between <rdf:Description ...> and </rdf:Description>.
fn extract_description_children(xmp: &str) -> Option<String> {
    let desc_start = xmp.find("<rdf:Description")?;
    let after = &xmp[desc_start..];

    // Find end of opening tag
    let mut in_quote = false;
    let mut tag_end = None;
    for (i, ch) in after.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            '>' if !in_quote => {
                // Check if it's self-closing
                if i > 0 && after.as_bytes()[i - 1] == b'/' {
                    return None; // Self-closing, no children
                }
                tag_end = Some(i + 1);
                break;
            }
            _ => {}
        }
    }

    let children_start = tag_end?;
    let close_tag = "</rdf:Description>";
    let children_end = after.find(close_tag)?;

    if children_start < children_end {
        Some(after[children_start..children_end].to_string())
    } else {
        None
    }
}

/// Remove <Container:Directory>...</Container:Directory> from XML string.
fn filter_container_directory(xml: &str) -> String {
    if let Some(start) = xml.find("<Container:Directory") {
        if let Some(end) = xml.find("</Container:Directory>") {
            let end_full = end + "</Container:Directory>".len();
            let mut result = xml[..start].to_string();
            result.push_str(&xml[end_full..]);
            return result;
        }
    }
    xml.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_preserves_namespaces() {
        let doc = XmpDocument::from_template().unwrap();
        let s = String::from_utf8(doc.to_bytes()).unwrap();
        assert!(s.contains("GCamera:MotionPhoto=\"1\""));
        assert!(s.contains("Item:Semantic=\"Primary\""));
        assert!(s.contains("Item:Mime=\"image/heic\""));
        assert!(s.contains("rdf:parseType=\"Resource\""));
    }

    #[test]
    fn test_set_timestamp() {
        let mut doc = XmpDocument::from_template().unwrap();
        doc.set_timestamp(1500000).unwrap();
        let s = String::from_utf8(doc.to_bytes()).unwrap();
        assert!(s.contains("MotionPhotoPresentationTimestampUs=\"1500000\""));
        assert!(!s.contains("TimestampUs=\"-1\""));
    }

    #[test]
    fn test_set_primary_mime() {
        let mut doc = XmpDocument::from_template().unwrap();
        doc.set_primary_mime("image/jpeg").unwrap();
        let s = String::from_utf8(doc.to_bytes()).unwrap();
        // Primary item should have jpeg, MotionPhoto should still have quicktime
        assert!(s.contains("Item:Mime=\"image/jpeg\""));
        assert!(s.contains("Item:Mime=\"video/quicktime\""));
    }

    #[test]
    fn test_set_motionphoto_length() {
        let mut doc = XmpDocument::from_template().unwrap();
        doc.set_motionphoto_length(12345).unwrap();
        let s = String::from_utf8(doc.to_bytes()).unwrap();
        assert!(s.contains("Item:Length=\"12345\""));
    }

    #[test]
    fn test_set_primary_padding() {
        let mut doc = XmpDocument::from_template().unwrap();
        doc.set_primary_padding(8).unwrap();
        let s = String::from_utf8(doc.to_bytes()).unwrap();
        assert!(s.contains("Item:Padding=\"8\""));
    }

    #[test]
    fn test_filter_container_directory() {
        let xml = "<foo/><Container:Directory><inner/></Container:Directory><bar/>";
        assert_eq!(filter_container_directory(xml), "<foo/><bar/>");
    }
}
