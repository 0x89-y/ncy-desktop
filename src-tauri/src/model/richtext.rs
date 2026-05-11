use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockType {
    #[serde(rename = "PARAGRAPH")]
    Paragraph,
    #[serde(rename = "HEADING1")]
    Heading1,
    #[serde(rename = "HEADING2")]
    Heading2,
    #[serde(rename = "HEADING3")]
    Heading3,
    #[serde(rename = "BULLET_LIST")]
    BulletList,
    #[serde(rename = "CODE_BLOCK")]
    CodeBlock,
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Paragraph
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichTextSpan {
    pub text: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default, rename = "inlineImage")]
    pub inline_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichTextBlock {
    #[serde(default, rename = "type")]
    pub block_type: BlockType,
    #[serde(default)]
    pub spans: Vec<RichTextSpan>,
}
