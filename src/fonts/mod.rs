//! 文字をフォント(映像)に変える

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::f32::math::{ceil, round};
use fontdue::{Font, FontSettings, Metrics};
use crate::{LINE_SPACING, ENABLE_LIGATURES};

#[derive(Default)]
/// テキストの映像のやつ
/// 理解できるかな？
///
/// # Items
/// * `start_x` - 開始地点
/// * `start_y` - 開始地点
/// * `width` - 縦の長さ
/// * `height` - 横の長さ
/// * `advance_w` - 開けるべき縦の長さ
/// * `advance_h` - 開けるべき横の長さ
/// * `bitmap` - ビットマップ
pub struct Text {
    pub start_x: i32,
    pub start_y: i32,
    pub width: usize,
    pub height: usize,
    pub advance_w: f32,
    pub advance_h: f32,
    pub bitmap: Vec<u8>,
}

/// フォントを読み込む
/// # Args
/// * `bytes` - フォントのバイト列
/// # Returns
/// * `Font` - フォント
pub fn load_font(bytes: &[u8]) -> Font {
    let setting = FontSettings::default();
    Font::from_bytes(bytes, setting).expect("Failed to load font")
}

/// ある程度の制御文字を受け入れながら描画する
/// # Args
/// `font_bytes` - フォントのバイト列
/// `text` - 描画する文字
/// `px` - ピクセルサイズ
/// `start_x` - 開始地点
/// `start_y` - 開始地点
pub fn gets_control_character_supported(
    analyzed_text: &Vec<u16>,
    font_bytes: &[u8],
    text: &str,
    px: f32,
    start_x: i32,
    start_y: i32,
) -> Vec<Text> {
    let mut all_texts = Vec::new();
    let font_obj = load_font(font_bytes);

    let line_height = font_obj
        .horizontal_line_metrics(px)
        .map(|lm| {
            let h = lm.ascent + lm.descent + lm.line_gap;
            (ceil(h) as i32).max(1)
        })
        .unwrap_or(px as i32);

    let mut baseline_y: f32 = start_y as f32;
    let mut buf = String::new();
    let mut prev_cr = false;

    for ch in text.chars() {
        match ch {
            '\r' => { prev_cr = true; }
            '\n' => {
                if !buf.is_empty() {
                    all_texts.append(&mut gets_with_obj(&analyzed_text, &font_obj, font_bytes, &buf, px, start_x, round(baseline_y) as i32));
                    buf.clear();
                }
                baseline_y += line_height as f32 * LINE_SPACING;
                prev_cr = false;
            }
            c => {
                if prev_cr {
                    if !buf.is_empty() {
                        all_texts.append(&mut gets_with_obj(&analyzed_text, &font_obj, font_bytes, &buf, px, start_x, round(baseline_y) as i32));
                        buf.clear();
                    }
                    prev_cr = false;
                }
                buf.push(c);
            }
        }
    }

    if !buf.is_empty() {
        all_texts.append(&mut gets_with_obj(&analyzed_text, &font_obj, font_bytes, &buf, px, start_x, baseline_y as i32));
    }

    all_texts
}

/// 内部用：すでにロード済みのFontオブジェクトを使って描画データを作る
/// # Args
/// * `font_obj` - フォント
/// * `font_bytes` - フォントのバイト列
/// * `text` - 文字
/// * `px` - ピクセルサイズ
/// * `x` - 開始地点
/// * `y` - 開始地点
fn gets_with_obj(analyzed_text: &Vec<u16>, font_obj: &Font, _font_bytes: &[u8], _text: &str, px: f32, x: i32, y: i32) -> Vec<Text> {
    let mut ret = vec![];
    let mut pen_x = x;

    // ここでリガチャを考慮したIDリストを取得
    let glyph_ids = analyzed_text;
    for gid in glyph_ids {
        let (metrics, bitmap) = font_obj.rasterize_indexed(*gid, px);
        let t = internal_get(metrics, bitmap, px, pen_x, y);
        pen_x += t.advance_w as i32;
        ret.push(t);
    }

    ret
}


/// グラフメトリクスをTextにする
/// # Args
/// * `metrics` - メトリクス
/// * `bitmap` - ビットマップ
/// * `_px` - ピクセル
/// * `x` - 開始地点
/// * `y` - 開始地点
fn internal_get(
    metrics: Metrics,
    bitmap: Vec<u8>,
    _px: f32,
    x: i32,
    y: i32,
) -> Text {
    let mut ret = Text::default();
    ret.width = metrics.width;
    ret.height = metrics.height;
    ret.bitmap = bitmap;
    ret.start_x = x + metrics.xmin;
    ret.start_y = y - metrics.height as i32 - metrics.ymin;
    ret.advance_w = metrics.advance_width;
    ret.advance_h = metrics.advance_height;
    ret
}

/// 文字列を GlyphID の配列に変換する
/// ENABLE_LIGATURES が false の場合、rustybuzz は一切呼び出されない
///
/// # Args
/// * `font_obj` - フォント
/// * `font_bytes` - フォントバイト列
/// * `text` - テキスト
pub fn analyze_text(font_obj: &Font, font_bytes: &[u8], text: &str) -> Vec<u16> {
    if ENABLE_LIGATURES {
        use rustybuzz::{Face, UnicodeBuffer};
        let face = Face::from_slice(font_bytes, 0).expect("Failed to load face");
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        let glyph_buffer = rustybuzz::shape(&face, &[], buffer);
        glyph_buffer.glyph_infos().iter().map(|info| info.glyph_id as u16).collect()
    } else {
        text.chars().map(|c| font_obj.lookup_glyph_index(c)).collect()
    }
}
