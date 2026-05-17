#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryDetection {
    Text,
    Image,
    Binary,
}

impl BinaryDetection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Binary => "binary",
        }
    }

    pub fn classify(
        path: &str,
        stored_kind: Option<&str>,
        is_binary: bool,
        blob_bytes: Option<&[u8]>,
    ) -> Self {
        if let Some(kind) = stored_kind {
            match kind {
                "image" => return Self::Image,
                "text" if !is_binary => return Self::Text,
                _ => {}
            }
        }

        if let Some(bytes) = blob_bytes {
            if is_image_bytes(bytes) {
                return Self::Image;
            }
            if bytes.contains(&0) {
                return Self::Binary;
            }
        }

        if is_binary {
            if is_image_path(path) {
                return Self::Image;
            }
            return Self::Binary;
        }

        if is_image_path(path) {
            return Self::Image;
        }
        Self::Text
    }
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(
        lower.rsplit('.').next(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

fn is_image_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
        || bytes.starts_with(b"BM")
}
