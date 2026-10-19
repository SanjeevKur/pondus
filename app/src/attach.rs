// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Files attached to a chat message. Images go to the model as images (TigrimOS inlines the
//! images saved in its folder when the model takes them); documents go as their text: PDF,
//! Word, Excel and plain text files are read here and sent inline with the message.

use std::path::{Path, PathBuf};

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const TEXT_EXTS: &[&str] = &["txt", "md", "csv", "tsv", "json", "xml", "html", "htm", "ifc", "log", "yaml", "yml", "toml", "py", "rs"];
pub const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;
/// The most text sent of one document, characters.
const MAX_TEXT: usize = 60_000;

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn is_image(p: &Path) -> bool {
    IMAGE_EXTS.contains(&ext(p).as_str())
}

/// Whether Pondus opens the file as a model rather than attaching it.
pub fn is_model(p: &Path) -> bool {
    matches!(ext(p).as_str(), "pondus")
}

/// Whether a file can be attached, or why not.
pub fn check(p: &Path) -> Result<(), String> {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let meta = std::fs::metadata(p).map_err(|e| format!("{name}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{name} is not a file"));
    }
    if is_image(p) {
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(format!("{name} is larger than 12 MB"));
        }
        return Ok(());
    }
    let e = ext(p);
    if e == "pdf" || e == "docx" || e == "xlsx" || e == "xls" || e == "ods" || TEXT_EXTS.contains(&e.as_str()) {
        if meta.len() > 50 * 1024 * 1024 {
            return Err(format!("{name} is larger than 50 MB"));
        }
        return Ok(());
    }
    Err(format!("{name}: the agent reads images (PNG, JPEG, GIF, WebP, BMP) and documents (PDF, Word, Excel, CSV, text, JSON, IFC)"))
}

/// A document's text, cut to `MAX_TEXT` characters (with a note when cut).
pub fn document_text(p: &Path) -> Result<String, String> {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = match ext(p).as_str() {
        "pdf" => pdf_extract::extract_text(p).map_err(|e| format!("{name}: cannot read the PDF's text: {e}"))?,
        "xlsx" | "xls" | "ods" => sheets(p).map_err(|e| format!("{name}: {e}"))?,
        "docx" => docx(p).map_err(|e| format!("{name}: {e}"))?,
        _ => {
            let b = std::fs::read(p).map_err(|e| format!("{name}: {e}"))?;
            String::from_utf8_lossy(&b).into_owned()
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(format!("{name} has no text the agent can read (a scanned PDF? attach it as images instead)"));
    }
    if text.chars().count() > MAX_TEXT {
        let cut: String = text.chars().take(MAX_TEXT).collect();
        return Ok(format!("{cut}\n[… the rest of {name} is left out: it is longer than {MAX_TEXT} characters]"));
    }
    Ok(text)
}

/// Every sheet of a workbook as tab-separated rows.
fn sheets(p: &Path) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(p).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for name in wb.sheet_names().to_vec() {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        out += &format!("## Sheet: {name}\n");
        for row in range.rows() {
            let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
            if cells.iter().any(|c| !c.is_empty()) {
                out += &cells.join("\t");
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// A Word document's paragraphs (word/document.xml without its markup).
fn docx(p: &Path) -> Result<String, String> {
    use std::io::Read;
    let f = std::fs::File::open(p).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("not a Word document: {e}"))?;
    let mut xml = String::new();
    z.by_name("word/document.xml").map_err(|e| format!("not a Word document: {e}"))?.read_to_string(&mut xml).map_err(|e| e.to_string())?;
    let xml = xml.replace("</w:p>", "\n").replace("<w:tab/>", "\t").replace("<w:br/>", "\n");
    let mut out = String::new();
    let mut tag = false;
    for c in xml.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    Ok(out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'"))
}

/// The files copied into the agent's folder, and the note added to the message: documents'
/// text inline first, then the images' "saved at" notes (TigrimOS sends those images to the
/// model and drops the note text after them).
pub fn prepare(files: &[PathBuf], dir: &Path) -> Result<String, String> {
    let att = dir.join(".attachments");
    let _ = std::fs::remove_dir_all(&att);
    if files.is_empty() {
        return Ok(String::new());
    }
    std::fs::create_dir_all(&att).map_err(|e| format!("{}: {e}", att.display()))?;
    let (mut docs, mut imgs) = (String::new(), String::new());
    for (i, p) in files.iter().enumerate() {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        if is_image(p) {
            let to = att.join(format!("{}-{}", i + 1, name.replace(['[', ']'], "_")));
            std::fs::copy(p, &to).map_err(|e| format!("{name}: {e}"))?;
            imgs += &format!("\n[Attached file: {name} — saved at: {}]", to.display());
        } else {
            let text = document_text(p)?;
            docs += &format!("\n\n--- Attached file: {name} ---\n{text}\n--- End of {name} ---");
        }
    }
    Ok(format!("{docs}{imgs}"))
}

/// A small picture of an image file, for the chat.
pub fn thumbnail(p: &Path, max: u32) -> Option<eframe::egui::ColorImage> {
    let img = image::open(p).ok()?.thumbnail(max, max).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_become_text_and_images_notes() {
        let d = std::env::temp_dir().join(format!("pondus-attach-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        let csv = d.join("rooms.csv");
        std::fs::write(&csv, "room,area\nLiving,32\n").unwrap();
        let png = d.join("sketch.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])).save(&png).unwrap();
        let note = prepare(&[png.clone(), csv.clone()], &d.join("work")).unwrap();
        // the document's text comes before the image's note
        let (i, j) = (note.find("Living,32").unwrap(), note.find("[Attached file: sketch.png — saved at:").unwrap());
        assert!(i < j, "{note}");
        assert!(d.join("work/.attachments/1-sketch.png").is_file());
        std::fs::write(d.join("model.dwg"), b"AC1032").unwrap();
        assert!(check(&d.join("model.dwg")).unwrap_err().contains("documents"));
        assert!(check(&csv).is_ok() && check(&png).is_ok());
        assert!(thumbnail(&png, 64).is_some());
        let _ = std::fs::remove_dir_all(&d);
    }
}
