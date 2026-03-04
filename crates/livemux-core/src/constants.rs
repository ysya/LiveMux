/// Samsung SEF Tag IDs — 4-byte big-endian identifiers.
/// Order matters: iteration order determines binary layout.
pub const SAMSUNG_TAG_ORDER: &[(&str, [u8; 4])] = &[
    ("Image_UTC_Data",           [0x00, 0x00, 0x01, 0x0a]),
    ("MCC_Data",                 [0x00, 0x00, 0xa1, 0x0a]),
    ("Camera_Scene_Info",        [0x00, 0x00, 0x01, 0x0d]),
    ("Color_Display_P3",         [0x00, 0x00, 0xc1, 0x0c]),
    ("Camera_Capture_Mode_Info", [0x00, 0x00, 0x61, 0x0c]),
    ("MotionPhoto_Data",         [0x00, 0x00, 0x30, 0x0a]),
    ("MotionPhoto_Version",      [0x00, 0x00, 0x31, 0x0a]),
];

pub const SAMSUNG_SEFH_VERSION: i32 = 107;

pub const MPVD_BOX_SIZE: usize = 8;
pub const MPVD_BOX_NAME: &[u8; 4] = b"mpvd";

pub const SEFD_BOX_SIZE: usize = 8;
pub const SEFD_BOX_NAME: &[u8; 4] = b"sefd";

pub const XMP_TEMPLATE: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 5.1.0-jc003">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:GCamera="http://ns.google.com/photos/1.0/camera/"
        xmlns:Container="http://ns.google.com/photos/1.0/container/"
        xmlns:Item="http://ns.google.com/photos/1.0/container/item/"
        xmlns:HDRGainMap="http://ns.apple.com/HDRGainMap/1.0/"
      GCamera:MotionPhoto="1"
      GCamera:MotionPhotoVersion="1"
      GCamera:MotionPhotoPresentationTimestampUs="-1">
      <Container:Directory>
        <rdf:Seq>
          <rdf:li rdf:parseType="Resource">
            <Container:Item
              Item:Mime="image/heic"
              Item:Semantic="Primary"
              Item:Length="0"
              Item:Padding="8"/>
          </rdf:li>
          <rdf:li rdf:parseType="Resource">
            <Container:Item
              Item:Mime="video/quicktime"
              Item:Semantic="MotionPhoto"
              Item:Length="404"
              Item:Padding="0"/>
          </rdf:li>
        </rdf:Seq>
      </Container:Directory>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#;


/// Video signature bytes for detecting embedded video in motion photos.
pub const VIDEO_SIGS: &[&[u8]] = &[b"ftyp", b"wide"];
pub const NOT_VIDEO_SIGS: &[&[u8]] = &[b"ftypheic", b"ftypM4A", b"ftypavif"];
