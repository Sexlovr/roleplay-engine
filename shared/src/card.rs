//! Tavern character-card import (V1 / V2 / V3) — pure parsing, no I/O so it
//! builds for wasm too. The PNG `tEXt` extraction (chara/ccv3 → base64 → JSON)
//! happens client-side; this takes the resulting JSON string and normalizes it
//! into a [`NewCharacterReq`].
//!
//! Reference: character-card-spec-v2 (`chara_card_v2`/2.0) and v3
//! (`chara_card_v3`/3.0). V2/V3 nest the real fields under `data`; V1 is flat.

use serde_json::Value;

use crate::dto::NewCharacterReq;
use crate::types::LoreEntry;

// ---- PNG character-card extraction -----------------------------------------

/// Decode standard base64 (with optional padding/whitespace) into bytes.
/// Returns `None` on any invalid character so callers can fall back gracefully.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' || c == b' ' || c == b'\t' {
            continue;
        }
        let v = val(c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// Standard base64 encode (with padding). Used to turn an imported PNG card
/// into a `data:` URL avatar.
pub fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.len();
        let b0 = chunk[0] as u32;
        let b1 = if n > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if n > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        s.push(T[((triple >> 18) & 63) as usize] as char);
        s.push(T[((triple >> 12) & 63) as usize] as char);
        s.push(if n > 1 { T[((triple >> 6) & 63) as usize] as char } else { '=' });
        s.push(if n > 2 { T[(triple & 63) as usize] as char } else { '=' });
    }
    s
}

/// Extract the embedded character-card JSON from a Tavern PNG's `tEXt` chunks.
/// Looks for keyword `ccv3` (V3, preferred) then `chara` (V2), base64-decoding
/// the value. Returns the JSON string, or `None` if the PNG has no card data.
pub fn extract_png_card(bytes: &[u8]) -> Option<String> {
    // PNG signature.
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 8 || bytes[..8] != SIG {
        return None;
    }
    let mut pos = 8usize;
    let mut chara: Option<String> = None;
    let mut ccv3: Option<String> = None;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start.checked_add(len)?;
        // `+ 4` for the trailing CRC. Compare via saturating_sub so a crafted
        // chunk length can't overflow usize on 32-bit / wasm32 targets.
        if data_end > bytes.len().saturating_sub(4) {
            break; // truncated
        }
        if kind == b"tEXt" {
            let chunk = &bytes[data_start..data_end];
            // tEXt = keyword \0 text (both Latin-1 / ASCII here).
            if let Some(nul) = chunk.iter().position(|&b| b == 0) {
                let keyword = String::from_utf8_lossy(&chunk[..nul]).to_ascii_lowercase();
                let text = String::from_utf8_lossy(&chunk[nul + 1..]).into_owned();
                if keyword == "ccv3" {
                    ccv3 = base64_decode(&text).map(|b| String::from_utf8_lossy(&b).into_owned());
                } else if keyword == "chara" {
                    chara = base64_decode(&text).map(|b| String::from_utf8_lossy(&b).into_owned());
                }
            }
        }
        if kind == b"IEND" {
            break;
        }
        pos = data_end + 4; // skip CRC
    }
    ccv3.or(chara)
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|x| x.to_string())
}

/// First non-empty of several string keys.
fn first_s(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| s(v, k).filter(|x| !x.trim().is_empty()))
}

fn str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(|x| x.to_string()))
                .filter(|x| !x.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Derive a short one-line tagline from longer text (first sentence/line, capped).
fn derive_tagline(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let sentence = line.split(['.', '!', '?', '\n']).next().unwrap_or(line).trim();
    let pick = if sentence.is_empty() { line } else { sentence };
    let mut out: String = pick.chars().take(120).collect();
    if pick.chars().count() > 120 {
        out.push('\u{2026}');
    }
    out
}

/// Parse a `character_book` / `lorebook` object into our simplified entries.
fn parse_lorebook(data: &Value) -> Vec<LoreEntry> {
    let book = data
        .get("character_book")
        .or_else(|| data.get("lorebook"))
        .or_else(|| data.get("world_info"));
    let Some(entries) = book.and_then(|b| b.get("entries")) else {
        return Vec::new();
    };
    // `entries` may be an array or an object keyed by index.
    let iter: Vec<&Value> = match entries {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => o.values().collect(),
        _ => return Vec::new(),
    };
    iter.into_iter()
        .filter_map(|e| {
            let content = first_s(e, &["content", "entry"]).unwrap_or_default();
            if content.trim().is_empty() {
                return None;
            }
            let mut keys = str_array(e, "keys");
            if keys.is_empty() {
                keys = str_array(e, "key");
            }
            let enabled = e
                .get("enabled")
                .and_then(|x| x.as_bool())
                .unwrap_or(true);
            Some(LoreEntry { keys, content, enabled })
        })
        .collect()
}

/// Normalize a raw Tavern card (V1/V2/V3) JSON string into a create request.
/// Returns an error string if the JSON can't be parsed or has no usable name.
pub fn parse_card(json: &str, avatar_override: Option<String>) -> Result<NewCharacterReq, String> {
    let root: Value = serde_json::from_str(json.trim())
        .map_err(|e| format!("Not valid character-card JSON: {e}"))?;

    // Detect spec + locate the data object (V2/V3 nest under `data`).
    let spec = s(&root, "spec").unwrap_or_default();
    let data = root.get("data").filter(|d| d.is_object()).unwrap_or(&root);

    let spec_version = if spec == "chara_card_v3" {
        "3.0".to_string()
    } else if spec == "chara_card_v2" {
        "2.0".to_string()
    } else {
        s(&root, "spec_version").unwrap_or_default()
    };

    let name = first_s(data, &["name", "char_name"])
        .filter(|n| !n.trim().is_empty())
        .ok_or("Card has no character name.")?;

    let description = first_s(data, &["description"]).unwrap_or_default();
    let personality = first_s(data, &["personality", "personality_summary"]).unwrap_or_default();
    let scenario = first_s(data, &["scenario"]).unwrap_or_default();
    let first_message = first_s(data, &["first_mes", "first_message", "greeting"]).unwrap_or_default();
    let mes_example = first_s(data, &["mes_example", "example_dialogue"]).unwrap_or_default();
    let creator_notes = first_s(data, &["creator_notes", "creatorcomment", "creator_comment"]).unwrap_or_default();
    let system_prompt = first_s(data, &["system_prompt"]).unwrap_or_default();
    let post_history_instructions =
        first_s(data, &["post_history_instructions", "jailbreak"]).unwrap_or_default();
    let creator = first_s(data, &["creator", "author"]).unwrap_or_default();
    let tags = str_array(data, "tags");
    let alternate_greetings = str_array(data, "alternate_greetings");
    let lorebook = parse_lorebook(data);

    // Tagline: prefer a short creator note, else the first sentence of the
    // description, else the personality.
    let tagline = {
        let basis = [creator_notes.as_str(), description.as_str(), personality.as_str()]
            .into_iter()
            .find(|x| !x.trim().is_empty())
            .unwrap_or("");
        derive_tagline(basis)
    };

    Ok(NewCharacterReq {
        name,
        tagline: Some(tagline).filter(|s| !s.is_empty()),
        description: Some(description).filter(|s| !s.is_empty()),
        personality: Some(personality).filter(|s| !s.is_empty()),
        scenario: Some(scenario).filter(|s| !s.is_empty()),
        first_message: Some(first_message).filter(|s| !s.is_empty()),
        avatar: avatar_override.filter(|s| !s.trim().is_empty()),
        tags: Some(tags).filter(|t| !t.is_empty()),
        creator: Some(creator).filter(|s| !s.is_empty()),
        nsfw: Some(false),
        spec_version: Some(spec_version).filter(|s| !s.is_empty()),
        creator_notes: Some(creator_notes).filter(|s| !s.is_empty()),
        system_prompt: Some(system_prompt).filter(|s| !s.is_empty()),
        post_history_instructions: Some(post_history_instructions).filter(|s| !s.is_empty()),
        mes_example: Some(mes_example).filter(|s| !s.is_empty()),
        alternate_greetings: Some(alternate_greetings).filter(|g| !g.is_empty()),
        lorebook: Some(lorebook).filter(|l| !l.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_flat() {
        let json = r#"{
            "name": "Aria",
            "description": "A bard. Loves stories.",
            "personality": "Cheerful, witty",
            "scenario": "A tavern",
            "first_mes": "Hello traveler!",
            "mes_example": "<START>",
            "tags": ["fantasy","music"]
        }"#;
        let req = parse_card(json, None).unwrap();
        assert_eq!(req.name, "Aria");
        assert_eq!(req.personality.as_deref(), Some("Cheerful, witty"));
        assert_eq!(req.first_message.as_deref(), Some("Hello traveler!"));
        assert_eq!(req.tags.as_deref().unwrap(), &["fantasy".to_string(), "music".to_string()]);
        assert_eq!(req.tagline.as_deref(), Some("A bard"));
        assert_eq!(req.spec_version, None); // v1 has no spec
    }

    #[test]
    fn parses_v2_nested() {
        let json = r#"{
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "Kai",
                "description": "Stoic swordsman",
                "personality": "Quiet",
                "scenario": "Edge of the empire",
                "first_mes": "...",
                "mes_example": "",
                "creator_notes": "Be gentle",
                "system_prompt": "You are Kai.",
                "post_history_instructions": "Stay in character.",
                "alternate_greetings": ["Hi.", "Hey there."],
                "tags": ["oc"],
                "creator": "me",
                "character_book": {
                    "entries": [
                        {"keys": ["empire"], "content": "The empire fell long ago.", "enabled": true},
                        {"keys": [], "content": "", "enabled": true}
                    ]
                }
            }
        }"#;
        let req = parse_card(json, Some("data:img".into())).unwrap();
        assert_eq!(req.name, "Kai");
        assert_eq!(req.spec_version.as_deref(), Some("2.0"));
        assert_eq!(req.system_prompt.as_deref(), Some("You are Kai."));
        assert_eq!(req.post_history_instructions.as_deref(), Some("Stay in character."));
        assert_eq!(req.alternate_greetings.as_deref().unwrap().len(), 2);
        assert_eq!(req.avatar.as_deref(), Some("data:img"));
        let lore = req.lorebook.as_deref().unwrap();
        assert_eq!(lore.len(), 1); // empty-content entry dropped
        assert_eq!(lore[0].keys, vec!["empire".to_string()]);
    }

    #[test]
    fn parses_v3_spec() {
        let json = r#"{"spec":"chara_card_v3","spec_version":"3.0","data":{"name":"Nova","description":"AI"}}"#;
        let req = parse_card(json, None).unwrap();
        assert_eq!(req.spec_version.as_deref(), Some("3.0"));
        assert_eq!(req.name, "Nova");
    }

    #[test]
    fn rejects_nameless() {
        let json = r#"{"description":"no name here"}"#;
        assert!(parse_card(json, None).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_card("not json", None).is_err());
    }

    #[test]
    fn base64_roundtrip_decodes_ascii() {
        // "Hello" → SGVsbG8=
        assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello");
        // No padding, with embedded newline.
        assert_eq!(base64_decode("SGVs\nbG8").unwrap(), b"Hello");
        // Invalid char → None.
        assert!(base64_decode("@@@@").is_none());
    }

    /// Build a minimal PNG (sig + one tEXt chunk + IEND) with a CRC placeholder.
    fn make_png(keyword: &str, b64_text: &str) -> Vec<u8> {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut chunk = Vec::new();
        chunk.extend_from_slice(keyword.as_bytes());
        chunk.push(0);
        chunk.extend_from_slice(b64_text.as_bytes());
        out.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
        out.extend_from_slice(b"tEXt");
        out.extend_from_slice(&chunk);
        out.extend_from_slice(&[0, 0, 0, 0]); // fake CRC
        // IEND
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    #[test]
    fn extracts_chara_from_png() {
        let json = r#"{"name":"Zed"}"#;
        let b64 = base64_encode(json.as_bytes());
        let png = make_png("chara", &b64);
        let extracted = extract_png_card(&png).unwrap();
        assert_eq!(extracted, json);
        let req = parse_card(&extracted, None).unwrap();
        assert_eq!(req.name, "Zed");
    }

    #[test]
    fn base64_encode_decode_roundtrip() {
        let data = b"\x00\x01\x02\xff\xfe\x80sphinx of black quartz";
        let enc = base64_encode(data);
        assert_eq!(base64_decode(&enc).unwrap(), data);
    }

    #[test]
    fn ccv3_takes_precedence_over_chara() {
        let v2 = base64_encode(br#"{"name":"Old"}"#);
        let v3 = base64_encode(br#"{"name":"New"}"#);
        // chara chunk first, then ccv3.
        let mut png = make_png("chara", &v2);
        // Splice a ccv3 chunk in before IEND by rebuilding.
        let png3 = make_png("ccv3", &v3);
        // Take the ccv3 tEXt chunk (skip 8-byte sig) and insert before png's IEND.
        let chunk = &png3[8..png3.len() - 12]; // drop sig + IEND(12)
        let iend_pos = png.len() - 12;
        png.splice(iend_pos..iend_pos, chunk.iter().copied());
        let extracted = extract_png_card(&png).unwrap();
        assert_eq!(extracted, r#"{"name":"New"}"#);
    }

    #[test]
    fn non_png_returns_none() {
        assert!(extract_png_card(b"not a png").is_none());
    }
}
