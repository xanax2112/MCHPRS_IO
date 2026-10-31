use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("([a-zA-Z0-9§\\-:/]+\\.[a-zA-Z/0-9§\\-:_#]+(\\.[a-zA-Z/0-9.§\\-:#\\?\\+=_]+)?)")
        .unwrap()
});

fn is_valid_hex(ch: char) -> bool {
    ch.is_numeric() || ('a'..='f').contains(&ch) || ('A'..='F').contains(&ch)
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ColorCode {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
    Obfuscated,
    Bold,
    Strikethrough,
    Underline,
    Italic,
    Reset,
}

impl ColorCode {
    fn parse(code: char) -> Option<ColorCode> {
        Some(match code {
            '0' => ColorCode::Black,
            '1' => ColorCode::DarkBlue,
            '2' => ColorCode::DarkGreen,
            '3' => ColorCode::DarkAqua,
            '4' => ColorCode::DarkRed,
            '5' => ColorCode::DarkPurple,
            '6' => ColorCode::Gold,
            '7' => ColorCode::Gray,
            '8' => ColorCode::DarkGray,
            '9' => ColorCode::Blue,
            'a' => ColorCode::Green,
            'b' => ColorCode::Aqua,
            'c' => ColorCode::Red,
            'd' => ColorCode::LightPurple,
            'e' => ColorCode::Yellow,
            'f' => ColorCode::White,
            'k' => ColorCode::Obfuscated,
            'l' => ColorCode::Bold,
            'm' => ColorCode::Strikethrough,
            'n' => ColorCode::Underline,
            'o' => ColorCode::Italic,
            'r' => ColorCode::Reset,
            _ => return None,
        })
    }

    fn is_formatting(self) -> bool {
        use ColorCode::*;
        matches!(
            self,
            Obfuscated | Bold | Strikethrough | Underline | Italic | Reset
        )
    }

    /// `parse` 的逆运算：这个颜色码对应 `§` 后面的那个字符。
    pub fn code_char(self) -> char {
        use ColorCode::*;
        match self {
            Black => '0',
            DarkBlue => '1',
            DarkGreen => '2',
            DarkAqua => '3',
            DarkRed => '4',
            DarkPurple => '5',
            Gold => '6',
            Gray => '7',
            DarkGray => '8',
            Blue => '9',
            Green => 'a',
            Aqua => 'b',
            Red => 'c',
            LightPurple => 'd',
            Yellow => 'e',
            White => 'f',
            Obfuscated => 'k',
            Bold => 'l',
            Strikethrough => 'm',
            Underline => 'n',
            Italic => 'o',
            Reset => 'r',
        }
    }
}

/// Bedrock 的 `§` 码只有 16 色，十六进制颜色按 RGB 距离取最近的那个。
fn nearest_code_for_hex(hex: &str) -> ColorCode {
    use ColorCode::*;

    // (颜色, r, g, b)：Minecraft 的 16 个基础色
    const PALETTE: &[(ColorCode, u8, u8, u8)] = &[
        (Black, 0x00, 0x00, 0x00),
        (DarkBlue, 0x00, 0x00, 0xAA),
        (DarkGreen, 0x00, 0xAA, 0x00),
        (DarkAqua, 0x00, 0xAA, 0xAA),
        (DarkRed, 0xAA, 0x00, 0x00),
        (DarkPurple, 0xAA, 0x00, 0xAA),
        (Gold, 0xFF, 0xAA, 0x00),
        (Gray, 0xAA, 0xAA, 0xAA),
        (DarkGray, 0x55, 0x55, 0x55),
        (Blue, 0x55, 0x55, 0xFF),
        (Green, 0x55, 0xFF, 0x55),
        (Aqua, 0x55, 0xFF, 0xFF),
        (Red, 0xFF, 0x55, 0x55),
        (LightPurple, 0xFF, 0x55, 0xFF),
        (Yellow, 0xFF, 0xFF, 0x55),
        (White, 0xFF, 0xFF, 0xFF),
    ];

    let digits = hex.trim_start_matches('#');
    let parsed = (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten();
    let Some(rgb) = parsed else {
        return White;
    };
    let (r, g, b) = (
        ((rgb >> 16) & 0xFF) as i32,
        ((rgb >> 8) & 0xFF) as i32,
        (rgb & 0xFF) as i32,
    );

    PALETTE
        .iter()
        .min_by_key(|(_, pr, pg, pb)| {
            let dr = r - *pr as i32;
            let dg = g - *pg as i32;
            let db = b - *pb as i32;
            dr * dr + dg * dg + db * db
        })
        .map(|(color, _, _, _)| *color)
        .unwrap_or(White)
}

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TextColor {
    Hex(String),
    ColorCode(ColorCode),
}

impl Into<TextColor> for ColorCode {
    fn into(self) -> TextColor {
        TextColor::ColorCode(self)
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum ClickEventType {
    OpenUrl,
    // RunCommand,
    // SuggestCommand,
}

#[derive(Serialize, Debug, Clone)]
pub struct ClickEvent {
    action: ClickEventType,
    value: String,
}

/// This is only used for `TextComponent` serialize
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(field: &bool) -> bool {
    !*field
}

pub struct TextComponentBuilder {
    component: TextComponent,
}

impl TextComponentBuilder {
    pub fn new(text: String) -> Self {
        let component = TextComponent {
            text,
            ..Default::default()
        };
        Self { component }
    }

    pub fn color(mut self, color: TextColor) -> Self {
        self.component.color = Some(color);
        self
    }

    pub fn color_code(mut self, color: ColorCode) -> Self {
        self.component.color = Some(TextColor::ColorCode(color));
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.component.strikethrough = true;
        self
    }

    pub fn bold(mut self) -> Self {
        self.component.bold = true;
        self
    }

    pub fn finish(self) -> TextComponent {
        self.component
    }
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct TextComponent {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub underlined: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub obfuscated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<TextColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "clickEvent")]
    pub click_event: Option<ClickEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<TextComponent>,
}

impl TextComponent {
    /// 把组件渲染成客户端可直接显示的 **`§` legacy 文本**。
    ///
    /// Bedrock 的计分板、聊天等字段是纯字符串（没有 Java 那套 JSON 组件），
    /// 颜色只能降级成 `§` 码（BUG-032 R3）。这里只处理我们能产出的形态：
    /// 单层 `text` + `color` + 若干格式位，`extra` 递归拼接。
    ///
    /// `TextColor::Hex` 转成 `§` + 最近似的 16 色之一（Bedrock 的 `§` 码没有十六进制颜色；
    /// Java 的 `§x` 扩展写法在计分板这类字段里不可靠，所以按亮度粗略映射）。
    pub fn to_legacy_text(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        if let Some(color) = &self.color {
            match color {
                TextColor::ColorCode(code) => {
                    let _ = write!(out, "§{}", code.code_char());
                }
                TextColor::Hex(hex) => {
                    let _ = write!(out, "§{}", nearest_code_for_hex(hex).code_char());
                }
            }
        }
        if self.bold {
            out.push_str("§l");
        }
        if self.italic {
            out.push_str("§o");
        }
        if self.underlined {
            out.push_str("§n");
        }
        if self.strikethrough {
            out.push_str("§m");
        }
        if self.obfuscated {
            out.push_str("§k");
        }
        out.push_str(&self.text);
        for child in &self.extra {
            out.push_str(&child.to_legacy_text());
        }
        out
    }

    pub fn from_legacy_text(message: &str) -> Vec<TextComponent> {
        let mut components = Vec::new();

        let mut cur_component: TextComponent = Default::default();

        let mut chars = message.chars();
        'main_loop: while let Some(c) = chars.next() {
            if c == '&'
                && let Some(code) = chars.next()
            {
                if let Some(color) = ColorCode::parse(code) {
                    let make_new = !cur_component.text.is_empty();
                    if color.is_formatting() && make_new {
                        components.push(cur_component.clone());
                        cur_component.text.clear();
                    }
                    match color {
                        ColorCode::Bold => cur_component.bold = true,
                        ColorCode::Italic => cur_component.italic = true,
                        ColorCode::Underline => cur_component.underlined = true,
                        ColorCode::Strikethrough => cur_component.strikethrough = true,
                        ColorCode::Obfuscated => cur_component.obfuscated = true,
                        _ => {
                            components.push(cur_component);
                            cur_component = Default::default();
                            cur_component.color = Some(TextColor::ColorCode(color));
                        }
                    }
                    continue;
                }
                cur_component.text.push(c);
                cur_component.text.push(code);
                continue;
            }
            if c == '#' {
                let mut hex = String::from(c);
                for _ in 0..6 {
                    if let Some(c) = chars.next() {
                        hex.push(c);
                        if !is_valid_hex(c) {
                            cur_component.text += &hex;
                            continue 'main_loop;
                        }
                    } else {
                        cur_component.text += &hex;
                        continue 'main_loop;
                    }
                }
                components.push(cur_component);
                cur_component = Default::default();
                cur_component.color = Some(TextColor::Hex(hex));
                continue;
            }
            cur_component.text.push(c);
        }
        components.push(cur_component);

        // This code is stinky
        // Find URLs and add click action
        let mut new_components = Vec::with_capacity(components.len());
        for component in components {
            let mut last = 0;
            let text = &component.text;

            for match_ in URL_REGEX.find_iter(text) {
                let index = match_.start();
                let matched = match_.as_str();
                if last != index {
                    let mut new = component.clone();
                    new.text = String::from(&text[last..index]);
                    new_components.push(new);
                }
                let mut new = component.clone();
                new.text = matched.to_string();
                new.click_event = Some(ClickEvent {
                    action: ClickEventType::OpenUrl,
                    value: matched.to_string(),
                });
                new_components.push(new);
                last = index + matched.len();
            }
            if last < text.len() {
                let mut new = component.clone();
                new.text = String::from(&text[last..]);
                new_components.push(new);
            }
        }

        new_components
    }

    pub fn encode_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn is_text_only(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underlined
            && !self.strikethrough
            && !self.obfuscated
            && self.extra.is_empty()
            && self.color.is_none()
            && self.click_event.is_none()
    }
}

impl<S> From<S> for TextComponent
where
    S: Into<String>,
{
    fn from(value: S) -> Self {
        TextComponent {
            text: value.into(),
            ..Default::default()
        }
    }
}
