use crate::constants::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImageType {
    Heic,
    Jpg,
}

/// Builds the Samsung SEF (Samsung EF) binary trailer appended after image data.
///
/// The trailer format uses mixed endianness:
/// - SEFH directory fields: little-endian i32
/// - ISOBMFF box sizes (mpvd/sefd): big-endian i32
pub struct SamsungTags {
    video_bytes: Vec<u8>,
    video_size: usize,
    image_type: ImageType,
    image_size: usize,
    /// Ordered tag storage: (tag_name, tag_value_bytes).
    /// Only populated tags are stored; iteration follows SAMSUNG_TAG_ORDER.
    tags: Vec<(String, Vec<u8>)>,
}

impl SamsungTags {
    pub fn new(video_bytes: Vec<u8>, image_type: ImageType) -> Self {
        let video_size = video_bytes.len();
        let mut tags = Vec::new();

        // MotionPhoto_Version always present
        tags.push(("MotionPhoto_Version".to_string(), b"mpv3".to_vec()));

        // MotionPhoto_Data: for JPG embed video directly, for HEIC use 12-byte placeholder
        let motion_data = if image_type == ImageType::Jpg {
            video_bytes.clone()
        } else {
            // Dummy 12 bytes matching the final size: "mpv2" + BE(i32) + BE(i32)
            b"mpv2___.___."[..12].to_vec()
        };
        tags.push(("MotionPhoto_Data".to_string(), motion_data));

        Self {
            video_bytes,
            video_size,
            image_type,
            image_size: 0,
            tags,
        }
    }

    /// Set the final image size (after XMP embedding). Updates MotionPhoto_Data for HEIC.
    pub fn set_image_size(&mut self, image_size: usize) {
        self.image_size = image_size;
        if self.image_type == ImageType::Heic {
            let video_offset = (image_size + MPVD_BOX_SIZE) as i32;
            let mut mp_data = Vec::with_capacity(12);
            mp_data.extend_from_slice(b"mpv2");
            mp_data.extend_from_slice(&video_offset.to_be_bytes());
            mp_data.extend_from_slice(&(self.video_size as i32).to_be_bytes());
            self.set_tag("MotionPhoto_Data", mp_data);
        }
    }

    /// Returns the image padding value for XMP `Item:Padding` on Primary.
    /// For HEIC: MPVD_BOX_SIZE (8). For JPG: bytes before MotionPhoto_Data value.
    pub fn get_image_padding(&self) -> usize {
        if self.image_type == ImageType::Heic {
            return MPVD_BOX_SIZE;
        }
        // For JPG: accumulate bytes until we reach MotionPhoto_Data's value
        let mut size = 0usize;
        for (tag_name, tag_id) in SAMSUNG_TAG_ORDER {
            if let Some(tag_value) = self.find_tag(tag_name) {
                size += tag_id.len();            // 4-byte tag ID
                size += 4;                       // LE i32 name length field
                size += tag_name.len();          // tag name string
                if *tag_name == "MotionPhoto_Data" {
                    return size;
                }
                size += tag_value.len();         // tag value bytes
            }
        }
        0 // should never reach here
    }

    /// Returns the video size value for XMP `Item:Length` on MotionPhoto.
    ///
    /// This is the distance from the video start to end of file (footer_len - image_padding),
    /// not the raw video byte count. Google Photos uses `file_size - Item:Length` to locate
    /// the video start, so this value must include the Samsung SEFD trailer that follows
    /// the video data within the footer.
    pub fn get_video_size(&self) -> usize {
        self.video_footer().len() - self.get_image_padding()
    }

    /// Build the complete Samsung SEF binary trailer.
    pub fn video_footer(&self) -> Vec<u8> {
        // Phase 1: Build tag_data and compute offsets/lengths
        let mut tag_data: Vec<u8> = Vec::new();
        let mut active_tags: Vec<(&str, usize, usize)> = Vec::new(); // (name, offset, length)

        for (tag_name, tag_id) in SAMSUNG_TAG_ORDER {
            if let Some(tag_value) = self.find_tag(tag_name) {
                let mut entry = Vec::new();
                entry.extend_from_slice(tag_id);
                entry.extend_from_slice(&(tag_name.len() as i32).to_le_bytes());
                entry.extend_from_slice(tag_name.as_bytes());
                entry.extend_from_slice(tag_value);

                let entry_len = entry.len();
                tag_data.extend_from_slice(&entry);

                // Update cumulative reverse offsets for all active tags
                for item in active_tags.iter_mut() {
                    item.1 += entry_len; // add this entry's length to all preceding offsets
                }
                active_tags.push((*tag_name, entry_len, entry_len));
            }
        }

        // Phase 2: Build SEFH header (all little-endian)
        let mut sefh: Vec<u8> = Vec::new();
        sefh.extend_from_slice(b"SEFH");
        sefh.extend_from_slice(&SAMSUNG_SEFH_VERSION.to_le_bytes());
        sefh.extend_from_slice(&(active_tags.len() as i32).to_le_bytes());

        for (tag_name, offset, length) in &active_tags {
            // Find the tag ID for this tag name
            let tag_id = SAMSUNG_TAG_ORDER
                .iter()
                .find(|(n, _)| n == tag_name)
                .map(|(_, id)| id)
                .unwrap();
            sefh.extend_from_slice(tag_id);
            sefh.extend_from_slice(&(*offset as i32).to_le_bytes());
            sefh.extend_from_slice(&(*length as i32).to_le_bytes());
        }

        let sefh_len = sefh.len() as i32;
        sefh.extend_from_slice(&sefh_len.to_le_bytes());
        sefh.extend_from_slice(b"SEFT");

        // Phase 3: Assemble final result
        match self.image_type {
            ImageType::Heic => {
                let mut inner = Vec::new();
                inner.extend_from_slice(&self.video_bytes);
                // SEFD box: BE size + "sefd" + tag_data + sefh
                let sefd_size = (tag_data.len() + sefh.len() + SEFD_BOX_SIZE) as i32;
                inner.extend_from_slice(&sefd_size.to_be_bytes());
                inner.extend_from_slice(SEFD_BOX_NAME);
                inner.extend_from_slice(&tag_data);
                inner.extend_from_slice(&sefh);
                // Wrap in mpvd box
                let mpvd_size = (inner.len() + MPVD_BOX_SIZE) as i32;
                let mut result = Vec::with_capacity(inner.len() + MPVD_BOX_SIZE);
                result.extend_from_slice(&mpvd_size.to_be_bytes());
                result.extend_from_slice(MPVD_BOX_NAME);
                result.extend_from_slice(&inner);
                result
            }
            ImageType::Jpg => {
                let mut result = Vec::with_capacity(tag_data.len() + sefh.len());
                result.extend_from_slice(&tag_data);
                result.extend_from_slice(&sefh);
                result
            }
        }
    }

    fn find_tag(&self, name: &str) -> Option<&[u8]> {
        self.tags
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_slice())
    }

    fn set_tag(&mut self, name: &str, value: Vec<u8>) {
        if let Some(entry) = self.tags.iter_mut().find(|(n, _)| n == name) {
            entry.1 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heic_motionphoto_data_encoding() {
        let video = vec![0u8; 100];
        let mut tags = SamsungTags::new(video, ImageType::Heic);
        tags.set_image_size(1000);

        let footer = tags.video_footer();
        // Find "mpv2" in the sefd tag data (not in the video bytes)
        // The video bytes are all zeros, so "mpv2" only appears in tag_data
        let pos = footer
            .windows(4)
            .rposition(|w| w == b"mpv2")
            .expect("mpv2 marker not found");
        let offset_be = i32::from_be_bytes(footer[pos + 4..pos + 8].try_into().unwrap());
        let size_be = i32::from_be_bytes(footer[pos + 8..pos + 12].try_into().unwrap());
        assert_eq!(offset_be, 1008); // 1000 + MPVD_BOX_SIZE(8)
        assert_eq!(size_be, 100);
    }

    #[test]
    fn test_sefh_little_endian_version() {
        let video = vec![0u8; 50];
        let tags = SamsungTags::new(video, ImageType::Jpg);
        let footer = tags.video_footer();

        let pos = footer
            .windows(4)
            .position(|w| w == b"SEFH")
            .expect("SEFH not found");
        let version = i32::from_le_bytes(footer[pos + 4..pos + 8].try_into().unwrap());
        assert_eq!(version, 107);
    }

    #[test]
    fn test_footer_ends_with_seft() {
        let video = vec![0u8; 10];
        let tags = SamsungTags::new(video, ImageType::Jpg);
        let footer = tags.video_footer();
        assert!(footer.ends_with(b"SEFT"));

        let tags_heic = SamsungTags::new(vec![0u8; 10], ImageType::Heic);
        let footer_heic = tags_heic.video_footer();
        assert!(footer_heic.ends_with(b"SEFT"));
    }

    #[test]
    fn test_heic_starts_with_mpvd() {
        let video = vec![0u8; 10];
        let tags = SamsungTags::new(video, ImageType::Heic);
        let footer = tags.video_footer();
        assert_eq!(&footer[4..8], b"mpvd");
    }

    #[test]
    fn test_jpg_no_mpvd() {
        let video = vec![0u8; 10];
        let tags = SamsungTags::new(video, ImageType::Jpg);
        let footer = tags.video_footer();
        assert!(footer.windows(4).position(|w| w == b"mpvd").is_none());
    }

    #[test]
    fn test_tag_order_data_before_version() {
        let video = vec![0u8; 10];
        let tags = SamsungTags::new(video, ImageType::Jpg);
        let footer = tags.video_footer();
        let data_pos = footer
            .windows(16)
            .position(|w| w == b"MotionPhoto_Data")
            .unwrap();
        let version_pos = footer
            .windows(19)
            .position(|w| w == b"MotionPhoto_Version")
            .unwrap();
        assert!(data_pos < version_pos);
    }

    #[test]
    fn test_heic_image_padding_is_mpvd_box_size() {
        let tags = SamsungTags::new(vec![0u8; 10], ImageType::Heic);
        assert_eq!(tags.get_image_padding(), 8);
    }

    #[test]
    fn test_jpg_image_padding() {
        let tags = SamsungTags::new(vec![0u8; 10], ImageType::Jpg);
        let padding = tags.get_image_padding();
        // MotionPhoto_Data comes first in SAMSUNG_TAG_ORDER (among active tags):
        // tag_id(4) + name_len_field(4) + "MotionPhoto_Data"(16) = 24
        // But MotionPhoto_Version comes AFTER MotionPhoto_Data in SAMSUNG_TAG_ORDER,
        // so we DON'T see it before MotionPhoto_Data.
        // Wait - the order in SAMSUNG_TAG_ORDER is:
        // ..., MotionPhoto_Data, MotionPhoto_Version
        // So MotionPhoto_Data is reached first. padding = 4 + 4 + 16 = 24
        assert_eq!(padding, 24);
    }
}
