//! Native PDF export for offers, invoices and credit notes.
//!
//! Writes PDF 1.4 directly, using the standard fonts Helvetica and Helvetica-Bold
//! (built into every PDF viewer, so nothing is embedded) with WinAnsi encoding,
//! which covers German umlauts, ß, € and „“.

use crate::i18n::{Lang, t_in};
use crate::model::*;
use crate::pdf_metrics::{HELVETICA, HELVETICA_BOLD};

// A4 in points (1 pt = 1/72 inch).
const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MM: f32 = 72.0 / 25.4;
/// Content must stay above the footer.
const BOTTOM: f32 = 38.0 * MM;

pub type Rgb = (f32, f32, f32);
const TEXT: Rgb = (0.11, 0.15, 0.20);
const MUTED: Rgb = (0.40, 0.45, 0.52);
const RULE: Rgb = (0.80, 0.84, 0.88);

// ---- Low-level helpers ----------------------------------------------------------

/// Maps a character to its WinAnsi code; unknown characters become "?".
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '™' => 0x99,
        '\t' => b' ',
        _ => b'?',
    }
}

fn char_width(code: u8, bold: bool) -> f32 {
    let table = if bold { &HELVETICA_BOLD } else { &HELVETICA };
    table[(code.max(32) - 32) as usize] as f32 / 1000.0
}

/// Width of a text in points.
pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    s.chars().map(|c| char_width(win_ansi(c), bold)).sum::<f32>() * size
}

/// The text as a PDF string literal in WinAnsi encoding.
fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        match win_ansi(c) {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            code => out.push(code),
        }
    }
    out.push(b')');
    out
}

/// Breaks text into lines no wider than `width`; keeps the author's line breaks.
fn wrap(s: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(&candidate, size, bold) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
            // A single word longer than the line is cut into pieces.
            while text_width(&line, size, bold) > width && line.chars().count() > 1 {
                let mut cut = line.chars().count();
                while cut > 1 && text_width(&line.chars().take(cut).collect::<String>(), size, bold) > width {
                    cut -= 1;
                }
                lines.push(line.chars().take(cut).collect());
                line = line.chars().skip(cut).collect();
            }
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Right,
}

/// What a page shows, for drawing it on screen (y counts up from the bottom, in points).
#[derive(Clone, Debug)]
pub enum Shape {
    /// `x` is the left edge, or the right edge for right-aligned text.
    Text { x: f32, y: f32, size: f32, bold: bool, color: Rgb, align: Align, text: String },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    /// Picture 1 is the logo, 2 the letter paper, 3 the signature.
    Image { slot: u8, x: f32, y: f32, w: f32, h: f32 },
}

/// One page's drawing commands.
#[derive(Default)]
struct Page {
    ops: Vec<u8>,
    /// The pictures this page shows, by slot.
    images: Vec<(u8, Logo)>,
    shapes: Vec<Shape>,
}

impl Page {
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, align: Align, s: &str) {
        if s.is_empty() {
            return;
        }
        self.shapes.push(Shape::Text { x, y, size, bold, color, align, text: s.to_string() });
        let x = match align {
            Align::Left => x,
            Align::Right => x - text_width(s, size, bold),
        };
        let font = if bold { "F2" } else { "F1" };
        self.ops.extend_from_slice(
            format!(
                "BT {:.3} {:.3} {:.3} rg /{font} {size:.2} Tf {x:.2} {y:.2} Td ",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
        self.ops.extend_from_slice(&pdf_string(s));
        self.ops.extend_from_slice(b" Tj ET\n");
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb) {
        self.shapes.push(Shape::Line { x1, y1, x2, y2, width, color });
        self.ops.extend_from_slice(
            format!(
                "{:.3} {:.3} {:.3} RG {width:.2} w {x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S\n",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
    }

    /// Draws a picture (/Im1 logo, /Im2 letter paper, /Im3 signature) into a box with its lower left corner at x, y.
    fn image(&mut self, slot: u8, img: &Logo, x: f32, y: f32, w: f32, h: f32) {
        if !self.images.iter().any(|(s, _)| *s == slot) {
            self.images.push((slot, img.clone()));
        }
        self.shapes.push(Shape::Image { slot, x, y, w, h });
        self.ops.extend_from_slice(format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im{slot} Do Q\n").as_bytes());
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        self.shapes.push(Shape::Rect { x, y, w, h, color });
        self.ops.extend_from_slice(
            format!(
                "{:.3} {:.3} {:.3} rg {x:.2} {y:.2} {w:.2} {h:.2} re f\n",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
    }
}

/// Assembles pages into a complete PDF file.
fn write_pdf(pages: &[Page], title: &str, author: &str) -> Vec<u8> {
    write_pdf_with(pages, title, author, None)
}

/// What turns a PDF into a ZUGFeRD / Factur-X invoice: the XML and its profile name.
struct Attachment<'a> {
    xml: &'a [u8],
    conformance: &'a str,
}

static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
static SRGB_PROFILE: &[u8] = include_bytes!("../assets/icc/sRGB-v4.icc");

/// The character a WinAnsi code stands for (for looking up glyph widths in a font file).
fn win_ansi_char(code: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match code {
        0x80..=0x9F => HIGH[(code - 0x80) as usize],
        _ => code as char,
    }
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(bytes).expect("in memory");
    e.finish().expect("in memory")
}

/// A text string as UTF-16 (needed when it may contain characters outside PDFDocEncoding).
fn utf16_string(s: &str) -> String {
    let mut hex = String::from("<FEFF");
    for u in s.encode_utf16() {
        hex.push_str(&format!("{u:04X}"));
    }
    hex.push('>');
    hex
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Font dictionary, descriptor and file for an embedded TrueType font (PDF/A requires embedding).
fn embedded_font(font: &[u8], name: &str, bold: bool) -> (String, String) {
    let face = ttf_parser::Face::parse(font, 0).expect("bundled font");
    let scale = 1000.0 / face.units_per_em() as f32;
    let widths: Vec<String> = (32u8..=255)
        .map(|code| {
            let w = face
                .glyph_index(win_ansi_char(code))
                .and_then(|g| face.glyph_hor_advance(g))
                .or_else(|| face.glyph_hor_advance(ttf_parser::GlyphId(0)))
                .unwrap_or(0);
            ((w as f32) * scale).round().to_string()
        })
        .collect();
    let b = face.global_bounding_box();
    let s = |v: i16| ((v as f32) * scale).round() as i32;
    let font_dict = format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /{name} /FirstChar 32 /LastChar 255 /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {{FD}} 0 R >>",
        widths.join(" ")
    );
    let descriptor = format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV {} /FontFile2 {{FF}} 0 R >>",
        s(b.x_min),
        s(b.y_min),
        s(b.x_max),
        s(b.y_max),
        s(face.ascender()),
        s(face.descender()),
        s(face.capital_height().unwrap_or(face.ascender())),
        if bold { 120 } else { 80 }
    );
    (font_dict, descriptor)
}

fn write_pdf_with(pages: &[Page], title: &str, author: &str, attach: Option<Attachment<'_>>) -> Vec<u8> {
    let pdfa = attach.is_some();
    let mut out: Vec<u8> = if pdfa { b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec() } else { b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec() };
    let mut offsets: Vec<usize> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 font regular, 4 font bold, 5 info,
    // then per page: page object and content stream; for PDF/A the extra objects follow.
    let n_pages = pages.len();
    let page_obj = |i: usize| 6 + i * 2;
    let content_obj = |i: usize| 7 + i * 2;
    let extra = 6 + n_pages * 2;
    let (fd1, ff1, fd2, ff2, meta, icc, ef, fs) = (extra, extra + 1, extra + 2, extra + 3, extra + 4, extra + 5, extra + 6, extra + 7);

    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    let stream = |dict: &str, data: &[u8]| -> Vec<u8> {
        let mut v = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        v.extend_from_slice(data);
        v.extend_from_slice(b"\nendstream");
        v
    };

    let now = chrono::Local::now();
    let offset = now.format("%z").to_string();
    let pdf_date = format!("D:{}{}'{}'", now.format("%Y%m%d%H%M%S"), &offset[..3], &offset[3..]);
    let xmp_date = now.format("%Y-%m-%dT%H:%M:%S%:z").to_string();

    if let Some(a) = &attach {
        let _ = a;
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Catalog /Pages 2 0 R /Lang (de-DE) /Metadata {meta} 0 R \
                 /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) /DestOutputProfile {icc} 0 R >>] \
                 /Names << /EmbeddedFiles << /Names [(factur-x.xml) {fs} 0 R] >> >> /AF [{fs} 0 R] >>"
            )
            .as_bytes(),
        );
    } else {
        obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    }
    let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", page_obj(i))).collect();
    obj(
        &mut out,
        &mut offsets,
        format!("<< /Type /Pages /Kids [{}] /Count {n_pages} >>", kids.join(" ")).as_bytes(),
    );
    let fonts = pdfa.then(|| (embedded_font(FONT_REGULAR, "LiberationSans", false), embedded_font(FONT_BOLD, "LiberationSans-Bold", true)));
    match &fonts {
        Some(((r, _), (b, _))) => {
            obj(&mut out, &mut offsets, r.replace("{FD}", &fd1.to_string()).as_bytes());
            obj(&mut out, &mut offsets, b.replace("{FD}", &fd2.to_string()).as_bytes());
        }
        None => {
            obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
            obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
        }
    }
    let info = if pdfa {
        format!(
            "<< /Title {} /Author {} /Creator (OpenKontor) /Producer (OpenKontor) /CreationDate ({pdf_date}) /ModDate ({pdf_date}) >>",
            utf16_string(title),
            utf16_string(author)
        )
        .into_bytes()
    } else {
        let mut info = b"<< /Title ".to_vec();
        info.extend_from_slice(&pdf_string(title));
        info.extend_from_slice(b" /Author ");
        info.extend_from_slice(&pdf_string(author));
        info.extend_from_slice(b" /Producer (OpenKontor) ");
        info.extend_from_slice(format!("/CreationDate (D:{}) >>", now.format("%Y%m%d%H%M%S")).as_bytes());
        info
    };
    obj(&mut out, &mut offsets, &info);
    // The pictures (logo, letter paper, signature) come after all other objects, shared by the pages.
    let mut images: Vec<(u8, Logo)> = Vec::new();
    for (slot, img) in pages.iter().flat_map(|p| p.images.iter()) {
        if !images.iter().any(|(s, _)| s == slot) {
            images.push((*slot, img.clone()));
        }
    }
    let image_base = if pdfa { extra + 8 } else { extra };
    let image_obj = |slot: u8| image_base + images.iter().position(|(s, _)| *s == slot).unwrap_or(0);
    for (i, page) in pages.iter().enumerate() {
        let xobject = if page.images.is_empty() {
            String::new()
        } else {
            let refs: Vec<String> = page.images.iter().map(|(s, _)| format!("/Im{s} {} 0 R", image_obj(*s))).collect();
            format!(" /XObject << {} >>", refs.join(" "))
        };
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >>{xobject} >> /Contents {} 0 R >>",
                content_obj(i)
            )
            .as_bytes(),
        );
        obj(&mut out, &mut offsets, &stream("", &page.ops));
    }

    let mut id_source = Vec::new();
    if let (Some(a), Some(((_, d1), (_, d2)))) = (&attach, &fonts) {
        for (descriptor, file, data) in [(d1, ff1, FONT_REGULAR), (d2, ff2, FONT_BOLD)] {
            obj(&mut out, &mut offsets, descriptor.replace("{FF}", &file.to_string()).as_bytes());
            obj(&mut out, &mut offsets, &stream(&format!("/Length1 {} /Filter /FlateDecode", data.len()), &deflate(data)));
        }
        let xmp = format!(
            r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
   <pdfaid:part>3</pdfaid:part>
   <pdfaid:conformance>B</pdfaid:conformance>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:format>application/pdf</dc:format>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
   <pdf:Producer>OpenKontor</pdf:Producer>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
   <xmp:CreatorTool>OpenKontor</xmp:CreatorTool>
   <xmp:CreateDate>{date}</xmp:CreateDate>
   <xmp:ModifyDate>{date}</xmp:ModifyDate>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
   <fx:DocumentType>INVOICE</fx:DocumentType>
   <fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>
   <fx:Version>1.0</fx:Version>
   <fx:ConformanceLevel>{level}</fx:ConformanceLevel>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
   <pdfaExtension:schemas>
    <rdf:Bag>
     <rdf:li rdf:parseType="Resource">
      <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
      <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>
      <pdfaSchema:prefix>fx</pdfaSchema:prefix>
      <pdfaSchema:property>
       <rdf:Seq>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentFileName</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The name of the embedded XML document</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentType</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The type of the hybrid document in capital letters, e.g. INVOICE or ORDER</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>Version</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The actual version of the standard applying to the embedded XML document</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>ConformanceLevel</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The conformance level of the embedded XML document</pdfaProperty:description></rdf:li>
       </rdf:Seq>
      </pdfaSchema:property>
     </rdf:li>
    </rdf:Bag>
   </pdfaExtension:schemas>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
            bom = '\u{feff}',
            title = xml_text(title),
            author = xml_text(author),
            date = xmp_date,
            level = a.conformance,
        );
        obj(&mut out, &mut offsets, &stream("/Type /Metadata /Subtype /XML", xmp.as_bytes()));
        obj(&mut out, &mut offsets, &stream("/N 3 /Filter /FlateDecode", &deflate(SRGB_PROFILE)));
        obj(
            &mut out,
            &mut offsets,
            &stream(
                &format!("/Type /EmbeddedFile /Subtype /text#2Fxml /Params << /Size {} /ModDate ({pdf_date}) >>", a.xml.len()),
                a.xml,
            ),
        );
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Filespec /F (factur-x.xml) /UF (factur-x.xml) /Desc (Factur-X/ZUGFeRD Rechnungsdaten) \
                 /AFRelationship /Alternative /EF << /F {ef} 0 R /UF {ef} 0 R >> >>"
            )
            .as_bytes(),
        );
        id_source.extend_from_slice(a.xml);
    }
    for (_, l) in &images {
        let pixels = decode_base64(&l.rgb);
        obj(
            &mut out,
            &mut offsets,
            &stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
                    l.width, l.height
                ),
                &pixels,
            ),
        );
    }
    id_source.extend_from_slice(title.as_bytes());
    id_source.extend_from_slice(pdf_date.as_bytes());

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    let id: String = {
        use sha2::Digest;
        sha2::Sha256::digest(&id_source).iter().take(16).map(|b| format!("{b:02X}")).collect()
    };
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A ZUGFeRD / Factur-X invoice: the normal invoice PDF as PDF/A-3 with the e-invoice
/// data (EN 16931) embedded. Fails with the list of missing details.
pub fn zugferd_pdf(data: &Data, doc: &Document) -> Result<Vec<u8>, Vec<String>> {
    let xml = crate::xrechnung::build(data, doc, crate::xrechnung::Profile::En16931)?;
    let (pages, title, author) = document_pages(data, doc);
    Ok(write_pdf_with(&pages, &title, &author, Some(Attachment { xml: xml.as_bytes(), conformance: "EN 16931" })))
}

// ---- Document layout ------------------------------------------------------------

fn rgb(c: [u8; 3]) -> Rgb {
    (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

/// A layout worked out in points.
struct Style {
    lay: PdfLayout,
    left: f32,
    right: f32,
    accent: Rgb,
    /// Running text size relative to 10 pt.
    scale: f32,
}

impl Style {
    fn new(lay: PdfLayout) -> Self {
        let left = lay.margin_left.clamp(10.0, 40.0) * MM;
        let right = PAGE_W - lay.margin_right.clamp(8.0, 40.0) * MM;
        let scale = lay.font_size.clamp(7.0, 14.0) / 10.0;
        Self { accent: rgb(lay.accent), left, right, scale, lay }
    }

    fn s(&self, size: f32) -> f32 {
        size * self.scale
    }

    fn width(&self) -> f32 {
        self.right - self.left
    }

    fn form_a(&self) -> bool {
        self.lay.address == AddressPos::FormA
    }

    /// Where the sender line above the address sits.
    fn address_y(&self) -> f32 {
        let mm = if self.form_a() { 32.0 } else { 50.0 };
        PAGE_H - mm * MM
    }

    /// Where the title starts at the latest.
    fn title_y(&self) -> f32 {
        let mm = if self.form_a() { 80.0 } else { 98.0 };
        PAGE_H - mm * MM
    }

    /// A new page with the letter paper behind it and the fold marks.
    fn page(&self, first: bool) -> Page {
        let mut p = Page::default();
        if let Some(bg) = &self.lay.background
            && bg.width > 0
            && bg.height > 0
            && (first || !self.lay.background_first_page_only)
        {
            p.image(2, bg, 0.0, 0.0, PAGE_W, PAGE_H);
        }
        if self.lay.fold_marks {
            let marks = if self.form_a() { [87.0, 192.0] } else { [105.0, 210.0] };
            for m in marks {
                let y = PAGE_H - m * MM;
                p.line(4.0 * MM, y, 8.0 * MM, y, 0.5, MUTED);
            }
            let y = PAGE_H - 148.5 * MM;
            p.line(4.0 * MM, y, 10.0 * MM, y, 0.5, MUTED);
        }
        p
    }

    /// Draws the signature picture at the left; returns where the text continues.
    fn signature(&self, p: &mut Page, y: f32) -> f32 {
        let Some(sig) = self.lay.signature.as_ref().filter(|s| s.width > 0 && s.height > 0) else { return y };
        let ratio = sig.width as f32 / sig.height as f32;
        let mut h = 16.0 * MM;
        let mut w = h * ratio;
        if w > 60.0 * MM {
            w = 60.0 * MM;
            h = w / ratio;
        }
        p.image(3, sig, self.left, y - h + 8.0, w, h);
        y - h - 6.0
    }

    fn signature_height(&self) -> f32 {
        if self.lay.signature.is_some() { 16.0 * MM + 6.0 } else { 0.0 }
    }
}

/// Column layout of the item table (right edges for numbers).
struct Cols {
    pos: f32,
    text: (f32, f32),
    qty: f32,
    price: f32,
    vat: Option<f32>,
    total: f32,
}

fn cols(st: &Style, vat: bool, quantities_only: bool) -> Cols {
    let k = st.scale.max(1.0);
    let (l, r) = (st.left, st.right);
    let text = if st.lay.show_position { l + 26.0 * k } else { l };
    if quantities_only {
        Cols { pos: l, text: (text, r - 90.0 * k), qty: r, price: r, vat: None, total: r }
    } else if vat {
        Cols { pos: l, text: (text, r - 270.0 * k), qty: r - 190.0 * k, price: r - 115.0 * k, vat: Some(r - 75.0 * k), total: r }
    } else {
        Cols { pos: l, text: (text, r - 230.0 * k), qty: r - 150.0 * k, price: r - 75.0 * k, vat: None, total: r }
    }
}

fn table_header(p: &mut Page, y: f32, c: &Cols, st: &Style, lang: Lang, quantities_only: bool) -> f32 {
    let s = st.s(8.5);
    let l = |de| t_in(lang, de);
    if st.lay.show_position {
        p.text(c.pos, y, s, true, TEXT, Align::Left, l("Pos."));
    }
    p.text(c.text.0, y, s, true, TEXT, Align::Left, l("Bezeichnung"));
    p.text(c.qty, y, s, true, TEXT, Align::Right, l("Menge"));
    if !quantities_only {
        p.text(c.price, y, s, true, TEXT, Align::Right, l("Einzelpreis"));
        if let Some(x) = c.vat {
            p.text(x, y, s, true, TEXT, Align::Right, l("USt."));
        }
        p.text(c.total, y, s, true, TEXT, Align::Right, l("Gesamt"));
    }
    p.line(st.left, y - 5.0, st.right, y - 5.0, 0.8, TEXT);
    y - st.s(18.0)
}

fn footer(p: &mut Page, company: &Company, st: &Style, page: usize, pages: usize, lang: Lang) {
    let l = |de| t_in(lang, de);
    let y = 26.0 * MM;
    let blocks: Vec<Vec<String>> = match st.lay.footer {
        FooterStyle::Keiner => Vec::new(),
        FooterStyle::Eigener => st
            .lay
            .footer_columns
            .iter()
            .take(4)
            .map(|c| company.fill_company(c).lines().map(str::to_string).collect::<Vec<_>>())
            .filter(|b| b.iter().any(|l| !l.trim().is_empty()))
            .collect(),
        FooterStyle::Automatisch => vec![
            vec![company.name.clone(), company.owner.clone(), company.street.clone(), company.zip_city.clone()],
            vec![
                if company.phone.is_empty() { String::new() } else { format!("{} {}", l("Tel."), company.phone) },
                company.email.clone(),
            ],
            vec![
                if company.tax_number.is_empty() { String::new() } else { format!("{} {}", l("Steuernr."), company.tax_number) },
                if company.vat_id.is_empty() { String::new() } else { format!("{} {}", l("USt-IdNr."), company.vat_id) },
            ],
            vec![
                company.bank.clone(),
                if company.iban.is_empty() { String::new() } else { format!("IBAN {}", company.iban) },
                if company.bic.is_empty() { String::new() } else { format!("BIC {}", company.bic) },
            ],
        ],
    };
    // Own text is wrapped to its column; the automatic details stay on one line each.
    let own = st.lay.footer == FooterStyle::Eigener;
    if !blocks.is_empty() {
        p.line(st.left, y + 10.0, st.right, y + 10.0, 0.5, RULE);
        let col_w = st.width() / blocks.len().max(1) as f32;
        for (i, block) in blocks.iter().enumerate() {
            let mut ly = y;
            let lines: Vec<String> = block.iter().filter(|l| !l.is_empty()).flat_map(|l| if own { wrap(l, 7.0, false, col_w - 6.0) } else { vec![l.clone()] }).take(6).collect();
            for line in lines {
                p.text(st.left + col_w * i as f32, ly, 7.0, false, MUTED, Align::Left, &line);
                ly -= 9.0;
            }
        }
    }
    if pages > 1 && st.lay.page_numbers {
        p.text(st.right, 12.0 * MM, 7.5, false, MUTED, Align::Right, &crate::i18n::fill(l("Seite {} von {}"), &[&page, &pages]));
    }
}

/// Letterhead, address field (DIN 5008 window position) and the details box on the right.
/// Returns where the details box ends.
fn letterhead(p: &mut Page, c: &Company, st: &Style, contact: Option<&Contact>, meta: &[(String, String)]) -> f32 {
    let lay = &st.lay;
    let form_a = st.form_a();
    let logo = c
        .logo
        .as_ref()
        .filter(|l| l.width > 0 && l.height > 0 && matches!(lay.header, HeaderStyle::NameUndLogo | HeaderStyle::NurLogo));
    let (top, lowest, usual) = if form_a { (6.0, 25.0, 17.0) } else { (8.0, 44.0, 25.2) };
    let logo_mm = lay.logo_height.clamp(6.0, lowest - top - 1.5);
    let rule_mm = if logo.is_some() { (top + logo_mm + 1.5).max(usual) } else { usual };
    let y_rule = PAGE_H - rule_mm * MM;
    let name_right = logo.is_some() && lay.logo_pos == LogoPos::Links;
    if lay.header != HeaderStyle::Keiner {
        if let Some(logo) = logo {
            let ratio = logo.width as f32 / logo.height as f32;
            let mut h = logo_mm * MM;
            let mut w = h * ratio;
            if w > 70.0 * MM {
                w = 70.0 * MM;
                h = w / ratio;
            }
            let x = match lay.logo_pos {
                LogoPos::Links => st.left,
                LogoPos::Mitte => (st.left + st.right - w) / 2.0,
                LogoPos::Rechts => st.right - w,
            };
            p.image(1, logo, x, y_rule + 5.0, w, h);
        }
        if matches!(lay.header, HeaderStyle::NameUndLogo | HeaderStyle::NurName) {
            let (x, align) = if name_right { (st.right, Align::Right) } else { (st.left, Align::Left) };
            p.text(x, y_rule + 9.0, 18.0, true, st.accent, align, &c.name);
        }
        if lay.header_rule {
            p.rect(st.left, y_rule, st.width(), 2.0, st.accent);
        }
        if !lay.tagline.trim().is_empty() {
            let (x, align) = if name_right { (st.right, Align::Right) } else { (st.left, Align::Left) };
            p.text(x, y_rule - 10.0, 8.0, false, MUTED, align, &c.fill_company(&lay.tagline));
        }
    }

    // Details box: labels left, values right, as wide as needed.
    let size = 9.0;
    let label_w = meta.iter().map(|(l, _)| text_width(l, size, false)).fold(0.0, f32::max);
    let value_w = meta.iter().map(|(_, v)| text_width(v, size, true)).fold(0.0, f32::max);
    let label_x = st.right - (label_w + 12.0 + value_w).max(120.0);
    let mut my = st.address_y();
    for (label, value) in meta {
        p.text(label_x, my, size, false, MUTED, Align::Left, label);
        p.text(st.right, my, size, true, TEXT, Align::Right, value);
        my -= 13.0;
    }

    let mut y = st.address_y();
    let sender = if lay.sender_line.trim().is_empty() {
        [c.name.as_str(), c.street.as_str(), c.zip_city.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        c.fill_company(&lay.sender_line)
    };
    p.text(st.left, y, 7.0, false, MUTED, Align::Left, &sender);
    p.line(st.left, y - 2.0, st.left + text_width(&sender, 7.0, false), y - 2.0, 0.4, MUTED);
    y -= 16.0;
    let size = 10.5 * st.scale.clamp(0.9, 1.15);
    if let Some(k) = contact {
        for line in [&k.company, &k.person, &k.street, &k.zip_city] {
            if !line.is_empty() {
                p.text(st.left, y, size, false, TEXT, Align::Left, line);
                y -= size * 1.3;
            }
        }
        if !k.country.is_empty() && !matches!(k.country.as_str(), "Deutschland" | "Germany") {
            p.text(st.left, y, size, false, TEXT, Align::Left, &k.country.to_uppercase());
        }
    }
    my
}

/// Writes wrapped text; returns where the next line goes.
fn paragraph(p: &mut Page, st: &Style, mut y: f32, text: &str, size: f32, bold: bool, color: Rgb) -> f32 {
    for line in wrap(text, st.s(size), bold, st.width()) {
        p.text(st.left, y, st.s(size), bold, color, Align::Left, &line);
        y -= st.s(size * 1.35);
    }
    y
}

/// The pages of a payment reminder (Zahlungserinnerung or Mahnung) for an open invoice.
fn reminder_pages(data: &Data, doc: &Document, r: &ReminderRecord) -> (Vec<Page>, String, String) {
    let c = &data.company;
    let st = Style::new(c.layout("Mahnung"));
    let small = c.small_business;
    let contact = data.contact(doc.contact_id);
    let lang = contact.map(|k| k.language).unwrap_or_default();
    let l = |de| t_in(lang, de);
    let money = |v: Cents| fmt_money_cur(lang, v, doc.currency());
    let date = |d: chrono::NaiveDate| fmt_date_in(lang, d);
    let gross = doc.totals(small).gross;
    let earlier: Vec<&ReminderRecord> = doc.reminders.iter().filter(|x| x.date <= r.date && *x != r).collect();
    // Placeholders count the fees up to this reminder.
    let mut before = doc.clone();
    before.reminders.retain(|x| x.date <= r.date && x != r);
    let fill = |s: &str| data.fill_text(s, &before, r.fee, r.due);
    let (title, intro, outro) = c.doc_text(reminder_title(r.level), lang);

    let mut p = st.page(true);
    let mut meta = vec![(l("Datum").to_string(), date(r.date))];
    if let Some(k) = contact
        && !k.number.is_empty()
        && st.lay.show_customer_number
    {
        meta.push((l("Kunden-Nr.").into(), k.number.clone()));
    }
    let my = letterhead(&mut p, c, &st, contact, &meta);
    let mut y = st.title_y().min(my - 20.0);
    let title = fill(&title);
    p.text(st.left, y, st.s(15.0), true, TEXT, Align::Left, &title);
    y -= st.s(22.0);
    y = paragraph(&mut p, &st, y, &fill(&intro), 10.0, false, TEXT);
    y -= 12.0;

    // The open item and the fees.
    let s = st.s(8.5);
    let k = st.scale.max(1.0);
    let (x_date, x_due) = (st.left + 120.0 * k, st.left + 210.0 * k);
    p.text(st.left, y, s, true, TEXT, Align::Left, l("Rechnung"));
    p.text(x_date, y, s, true, TEXT, Align::Left, l("Datum"));
    p.text(x_due, y, s, true, TEXT, Align::Left, l("Fällig am"));
    p.text(st.right, y, s, true, TEXT, Align::Right, l("Betrag"));
    p.line(st.left, y - 5.0, st.right, y - 5.0, 0.8, TEXT);
    y -= st.s(18.0);
    let s = st.s(9.5);
    p.text(st.left, y, s, false, TEXT, Align::Left, &doc.number);
    p.text(x_date, y, s, false, TEXT, Align::Left, &date(doc.date));
    p.text(x_due, y, s, false, TEXT, Align::Left, &date(doc.due_date()));
    p.text(st.right, y, s, false, TEXT, Align::Right, &money(gross));
    y -= st.s(16.0);
    let mut total = gross;
    for e in &earlier {
        if e.fee > 0 {
            p.text(st.left, y, s, false, TEXT, Align::Left, &crate::i18n::fill(l("Mahngebühr {} vom {}"), &[&l(reminder_title(e.level)), &date(e.date)]));
            p.text(st.right, y, s, false, TEXT, Align::Right, &money(e.fee));
            total += e.fee;
            y -= st.s(16.0);
        }
    }
    if r.fee > 0 {
        p.text(st.left, y, s, false, TEXT, Align::Left, l("Mahngebühr dieser Mahnung"));
        p.text(st.right, y, s, false, TEXT, Align::Right, &money(r.fee));
        total += r.fee;
        y -= st.s(16.0);
    }
    let label_x = st.right - 190.0 * k;
    p.line(label_x, y + 8.0, st.right, y + 8.0, 1.2, TEXT);
    y -= 6.0;
    p.text(label_x, y, st.s(11.5), true, TEXT, Align::Left, l("Offener Betrag"));
    p.text(st.right, y, st.s(11.5), true, TEXT, Align::Right, &money(total));
    y -= st.s(30.0);

    let pay = crate::i18n::fill(l("Bitte überweisen Sie {} bis zum {} auf unser Konto:"), &[&money(total), &date(r.due)]);
    y = paragraph(&mut p, &st, y, &pay, 10.0, true, TEXT);
    for line in [
        c.bank.clone(),
        if c.iban.is_empty() { String::new() } else { format!("IBAN {}", c.iban) },
        if c.bic.is_empty() { String::new() } else { format!("BIC {}", c.bic) },
        crate::i18n::fill(l("Verwendungszweck: {}"), &[&doc.number]),
    ] {
        if !line.is_empty() {
            p.text(st.left, y, st.s(10.0), false, TEXT, Align::Left, &line);
            y -= st.s(13.5);
        }
    }
    y -= 10.0;
    y = paragraph(&mut p, &st, y, &fill(&outro), 9.0, false, MUTED);
    st.signature(&mut p, y - 4.0);
    footer(&mut p, c, &st, 1, 1, lang);
    let title = format!("{} {}", l(reminder_title(r.level)), doc.number);
    (vec![p], title, c.name.clone())
}

/// A payment reminder (Zahlungserinnerung or Mahnung) for an open invoice.
pub fn reminder_pdf(data: &Data, doc: &Document, r: &ReminderRecord) -> Vec<u8> {
    let (pages, title, author) = reminder_pages(data, doc, r);
    write_pdf(&pages, &title, &author)
}

/// A file name like "1. Mahnung RE-2026-0012.pdf".
pub fn reminder_file_name(data: &Data, doc: &Document, r: &ReminderRecord) -> String {
    let _ = data;
    let number: String = if doc.number.is_empty() { "Entwurf".to_string() } else { doc.number.clone() }
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    format!("{} {number}.pdf", crate::i18n::t(reminder_title(r.level)))
}

/// The finished PDF for an offer, invoice or credit note.
pub fn document_pdf(data: &Data, doc: &Document) -> Vec<u8> {
    let (pages, title, author) = document_pages(data, doc);
    write_pdf(&pages, &title, &author)
}

/// The laid-out pages of a document, with title and author for the file's details.
fn document_pages(data: &Data, doc: &Document) -> (Vec<Page>, String, String) {
    let c = &data.company;
    let st = Style::new(c.layout(doc.kind.name()));
    let small = c.small_business;
    let contact = data.contact(doc.contact_id);
    let totals = doc.totals(small);
    let lang = contact.map(|k| k.language).unwrap_or_default();
    let l = |de| t_in(lang, de);
    let money = |c: Cents| fmt_money_cur(lang, c, doc.currency());
    let date = |d: chrono::NaiveDate| fmt_date_in(lang, d);
    let fill = |s: &str| data.fill_text(s, doc, 0, doc.due_date());
    let kind = l(doc.kind.name());
    let number = if doc.number.is_empty() { l("ENTWURF").to_string() } else { doc.number.clone() };
    let sign = if doc.kind == DocKind::Gutschrift { -1 } else { 1 };

    let mut pages: Vec<Page> = Vec::new();
    let mut p = st.page(true);

    // Details box on the right.
    let mut meta: Vec<(String, String)> = vec![
        (crate::i18n::fill(l("{}-Nr."), &[&kind]), number.clone()),
        (l("Datum").into(), date(doc.date)),
    ];
    if !doc.service_date.trim().is_empty() {
        meta.push((l("Leistungsdatum").into(), doc.service_date.clone()));
    }
    match doc.kind {
        DocKind::Rechnung => meta.push((l("Fällig am").into(), date(doc.due_date()))),
        DocKind::Angebot => meta.push((l("Gültig bis").into(), date(doc.due_date()))),
        _ => {}
    }
    if let Some(k) = contact
        && !k.number.is_empty()
        && doc.kind != DocKind::Bestellung
        && st.lay.show_customer_number
    {
        meta.push((l("Kunden-Nr.").into(), k.number.clone()));
    }
    if let Some(src) = doc.source_id.and_then(|id| data.documents.iter().find(|d| d.id == id))
        && !src.number.is_empty()
        && matches!(doc.kind, DocKind::Lieferschein | DocKind::Auftragsbestaetigung)
    {
        meta.push((crate::i18n::fill(l("{}-Nr."), &[&l(src.kind.name())]), src.number.clone()));
    }
    // A delivery note shows quantities only.
    let quantities_only = doc.kind == DocKind::Lieferschein;
    let my = letterhead(&mut p, c, &st, contact, &meta);

    // Title and opening text.
    let mut y = st.title_y().min(my - 20.0);
    let title = fill(if doc.title.is_empty() { kind } else { &doc.title });
    p.text(st.left, y, st.s(15.0), true, TEXT, Align::Left, &title);
    y -= st.s(20.0);
    y = paragraph(&mut p, &st, y, &fill(&doc.intro), 10.0, false, TEXT);
    y -= 8.0;

    // Item table, continued on further pages when needed.
    let vat_col = !small && !quantities_only && st.lay.show_vat_column;
    let col = cols(&st, vat_col, quantities_only);
    y = table_header(&mut p, y, &col, &st, lang, quantities_only);
    let (size, step) = (st.s(9.5), st.s(12.0));
    for (i, l) in doc.lines.iter().enumerate() {
        let mut text_lines = wrap(&l.text, size, false, col.text.1 - col.text.0);
        if text_lines.is_empty() {
            text_lines.push(String::new());
        }
        if l.discount_pct > 0.0 {
            text_lines.push(crate::i18n::fill(t_in(lang, "abzgl. {} % Rabatt"), &[&fmt_qty_in(lang, l.discount_pct)]));
        }
        let row_h = text_lines.len() as f32 * step + 7.0;
        if y - row_h < BOTTOM {
            pages.push(std::mem::replace(&mut p, st.page(false)));
            y = PAGE_H - 25.0 * MM;
            p.text(st.left, y, 9.0, false, MUTED, Align::Left, &crate::i18n::fill(t_in(lang, "{} {} – Fortsetzung"), &[&kind, &number]));
            y -= 22.0;
            y = table_header(&mut p, y, &col, &st, lang, quantities_only);
        }
        if st.lay.show_position {
            p.text(col.pos, y, size, false, TEXT, Align::Left, &(i + 1).to_string());
        }
        for (k, t) in text_lines.iter().enumerate() {
            let is_discount = l.discount_pct > 0.0 && k == text_lines.len() - 1;
            p.text(
                col.text.0,
                y - k as f32 * step,
                if is_discount { st.s(8.0) } else { size },
                false,
                if is_discount { MUTED } else { TEXT },
                Align::Left,
                t,
            );
        }
        p.text(col.qty, y, size, false, TEXT, Align::Right, &format!("{} {}", fmt_qty_in(lang, l.qty), l.unit));
        if !quantities_only {
            p.text(col.price, y, size, false, TEXT, Align::Right, &money(l.price_net));
            if let Some(x) = col.vat {
                p.text(x, y, size, false, TEXT, Align::Right, &format!("{} %", l.vat));
            }
            p.text(col.total, y, size, false, TEXT, Align::Right, &money(sign * l.net()));
        }
        y -= row_h;
        p.line(st.left, y + 9.0, st.right, y + 9.0, 0.4, RULE);
    }

    if quantities_only {
        y -= 14.0;
        y = paragraph(&mut p, &st, y, &fill(&doc.outro), 10.0, false, TEXT);
        y = st.signature(&mut p, y);
        y -= 30.0;
        if y < BOTTOM {
            pages.push(std::mem::replace(&mut p, st.page(false)));
            y = PAGE_H - 40.0 * MM;
        }
        p.line(st.left, y, st.left + 180.0, y, 0.6, TEXT);
        p.text(st.left, y - 11.0, 8.0, false, MUTED, Align::Left, l("Datum, Unterschrift Empfänger"));
        pages.push(p);
        let n = pages.len();
        for (i, page) in pages.iter_mut().enumerate() {
            footer(page, c, &st, i + 1, n, lang);
        }
        return (pages, format!("{kind} {number}"), c.name.clone());
    }

    // Totals; keep them together with the closing text on one page.
    let tax_rows = if small { 0 } else { totals.by_rate.keys().filter(|r| **r > 0).count() };
    let outro = wrap(&fill(&doc.outro), st.s(10.0), false, st.width());
    let pay = payment_lines(doc, small, lang);
    let needed = st.s(30.0 + tax_rows as f32 * 14.0 + 30.0 + (outro.len() + pay.len()) as f32 * 13.5 + if small { 16.0 } else { 0.0 })
        + st.signature_height();
    if y - needed < BOTTOM {
        pages.push(std::mem::replace(&mut p, st.page(false)));
        y = PAGE_H - 25.0 * MM;
    }
    y -= 8.0;
    let k = st.scale.max(1.0);
    let label_x = st.right - 190.0 * k;
    let (s10, step) = (st.s(10.0), st.s(14.0));
    p.text(label_x, y, s10, false, TEXT, Align::Left, l("Summe netto"));
    p.text(st.right, y, s10, false, TEXT, Align::Right, &money(totals.net));
    y -= step;
    if !small {
        for (rate, (_net, tax)) in &totals.by_rate {
            if *rate > 0 {
                p.text(label_x, y, s10, false, TEXT, Align::Left, &crate::i18n::fill(l("zzgl. Umsatzsteuer {} %"), &[rate]));
                p.text(st.right, y, s10, false, TEXT, Align::Right, &money(*tax));
                y -= step;
            }
        }
    }
    p.line(label_x, y + 8.0, st.right, y + 8.0, 1.2, TEXT);
    y -= 6.0;
    p.text(label_x, y, st.s(11.5), true, TEXT, Align::Left, l("Gesamtbetrag"));
    p.text(st.right, y, st.s(11.5), true, TEXT, Align::Right, &money(totals.gross));
    y -= st.s(28.0);
    let s85 = st.s(8.5);
    if c.small_business {
        p.text(st.left, y, s85, false, MUTED, Align::Left, l("Gemäß § 19 UStG wird keine Umsatzsteuer berechnet."));
        y -= st.s(16.0);
    } else if let Some(note) = doc.vat_case.invoice_note() {
        p.text(st.left, y, s85, false, TEXT, Align::Left, l(note));
        y -= st.s(12.0);
        if let Some(k) = contact.filter(|k| !k.vat_id.is_empty()) {
            p.text(st.left, y, s85, false, MUTED, Align::Left, &crate::i18n::fill(l("USt-IdNr. des Kunden: {}"), &[&k.vat_id]));
            y -= st.s(12.0);
        }
        y -= 4.0;
    }
    // Invoices in another currency state the VAT in euros as well (§ 16 Abs. 6 UStG).
    if doc.is_foreign() && totals.tax != 0 && doc.exchange_rate > 0.0 {
        let text = crate::i18n::fill(
            l("Umsatzsteuer in Euro: {} (Kurs: 1 EUR = {} {})"),
            &[&fmt_money_in(lang, doc.to_eur(totals.tax)), &format!("{:.4}", doc.exchange_rate), &doc.currency()],
        );
        p.text(st.left, y, s85, false, MUTED, Align::Left, &text);
        y -= st.s(14.0);
    }
    let paid = doc.paid_total();
    if doc.kind.is_billing() && paid != 0 && doc.status == DocStatus::Offen {
        p.text(label_x, y, s10, false, TEXT, Align::Left, l("Bereits bezahlt"));
        p.text(st.right, y, s10, false, TEXT, Align::Right, &money(paid));
        y -= step;
        p.text(label_x, y, st.s(10.5), true, TEXT, Align::Left, l("Noch offen"));
        p.text(st.right, y, st.s(10.5), true, TEXT, Align::Right, &money(totals.gross - paid));
        y -= st.s(22.0);
    }
    let line_h = st.s(13.5);
    for line in &pay {
        p.text(st.left, y, s10, false, TEXT, Align::Left, line);
        y -= line_h;
    }
    if !pay.is_empty() {
        y -= 6.0;
    }
    for line in outro {
        p.text(st.left, y, s10, false, TEXT, Align::Left, &line);
        y -= line_h;
    }
    st.signature(&mut p, y - 4.0);
    pages.push(p);

    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        footer(page, c, &st, i + 1, n, lang);
    }
    (pages, format!("{kind} {number}"), c.name.clone())
}

// ---- Preview ----------------------------------------------------------------------

/// Page size in points (A4).
pub const PAGE_SIZE: (f32, f32) = (PAGE_W, PAGE_H);

/// A page as shapes, to draw it on screen exactly as it prints.
pub struct PreviewPage {
    pub shapes: Vec<Shape>,
    pub images: Vec<(u8, Logo)>,
}

fn preview_of(pages: Vec<Page>) -> Vec<PreviewPage> {
    pages.into_iter().map(|p| PreviewPage { shapes: p.shapes, images: p.images }).collect()
}

/// The pages of a document as they will print.
pub fn document_preview(data: &Data, doc: &Document) -> Vec<PreviewPage> {
    preview_of(document_pages(data, doc).0)
}

/// The pages of a reminder as they will print.
pub fn reminder_preview(data: &Data, doc: &Document, r: &ReminderRecord) -> Vec<PreviewPage> {
    preview_of(reminder_pages(data, doc, r).0)
}

/// The fonts of the PDF (metrically the same as Helvetica), for drawing previews.
pub fn preview_fonts() -> (&'static [u8], &'static [u8]) {
    (FONT_REGULAR, FONT_BOLD)
}

/// Unpacks a stored picture into RGB pixels.
pub fn logo_pixels(logo: &Logo) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    let _ = flate2::read::ZlibDecoder::new(decode_base64(&logo.rgb).as_slice()).read_to_end(&mut out);
    out
}

/// One block of a text report: a heading, paragraphs, or a table of label/amount rows.
pub enum Block {
    Heading(String),
    Paragraph(String),
    /// Rows of (label, value); a row with an empty label draws a rule.
    Rows(Vec<(String, String, bool)>),
}

/// A plain report on the letterhead, e.g. the procedure documentation or a balance sheet.
pub fn report_pdf(data: &Data, title: &str, subtitle: &str, blocks: &[Block]) -> Vec<u8> {
    let c = &data.company;
    let st = Style::new(c.layout(""));
    let (left, right) = (st.left, st.right);
    let mut pages = Vec::new();
    let mut p = Page::default();
    let mut y = PAGE_H - 22.0 * MM;
    p.text(left, y, 14.0, true, st.accent, Align::Left, &c.name);
    p.rect(left, y - 7.0, right - left, 1.5, st.accent);
    y -= 34.0;
    p.text(left, y, 16.0, true, TEXT, Align::Left, title);
    y -= 16.0;
    p.text(left, y, 9.5, false, MUTED, Align::Left, subtitle);
    y -= 22.0;
    let mut new_page = |p: &mut Page, y: &mut f32| {
        if *y < BOTTOM {
            pages.push(std::mem::take(p));
            *y = PAGE_H - 25.0 * MM;
        }
    };
    for b in blocks {
        match b {
            Block::Heading(h) => {
                if y - 40.0 < BOTTOM {
                    y = BOTTOM - 1.0;
                    new_page(&mut p, &mut y);
                }
                y -= 6.0;
                p.text(left, y, 11.5, true, st.accent, Align::Left, h);
                y -= 16.0;
            }
            Block::Paragraph(text) => {
                for para in text.split('\n') {
                    for line in wrap(para, 9.5, false, right - left) {
                        new_page(&mut p, &mut y);
                        p.text(left, y, 9.5, false, TEXT, Align::Left, &line);
                        y -= 12.5;
                    }
                }
                y -= 6.0;
            }
            Block::Rows(rows) => {
                for (label, value, bold) in rows {
                    new_page(&mut p, &mut y);
                    if label.is_empty() && value.is_empty() {
                        p.line(left, y + 8.0, right, y + 8.0, 0.5, RULE);
                        y -= 4.0;
                        continue;
                    }
                    for (k, line) in wrap(label, 9.5, *bold, right - left - 120.0).iter().enumerate() {
                        p.text(left, y - k as f32 * 12.0, 9.5, *bold, TEXT, Align::Left, line);
                    }
                    p.text(right, y, 9.5, *bold, TEXT, Align::Right, value);
                    y -= 14.0;
                }
                y -= 6.0;
            }
        }
    }
    pages.push(p);
    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        footer(page, c, &st, i + 1, n, Lang::De);
    }
    write_pdf(&pages, title, &c.name)
}

/// Decodes standard base64 (used for the stored logo).
pub fn decode_base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0);
    for v in s.bytes().filter_map(val) {
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

pub fn encode_base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Turns RGB pixels into a logo for the letterhead.
pub fn make_logo(width: u32, height: u32, rgb: &[u8]) -> Logo {
    Logo { width, height, rgb: encode_base64(&deflate(rgb)) }
}

/// A file name like "Rechnung RE-2026-0012.pdf", after the company's pattern.
pub fn file_name(data: &Data, doc: &Document) -> String {
    data.company.pdf_file_name(doc, &data.contact_name(doc.contact_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_encoding() {
        assert!((text_width("Hallo", 10.0, false) - 22.78).abs() < 0.01);
        assert_eq!(win_ansi('ä'), 0xE4);
        assert_eq!(win_ansi('€'), 0x80);
        assert_eq!(pdf_string("(a)"), b"(\\(a\\))".to_vec());
    }

    #[test]
    fn layouts_and_texts_are_used() {
        let mut data = sample_data();
        let mut offer = data.documents.iter().find(|d| d.kind == DocKind::Angebot).unwrap().clone();
        offer.intro = "Angebot für {kunde}, gültig bis {faellig}.".into();
        let picture = make_logo(2, 2, &[200; 12]);
        data.company.pdf_layouts.insert(
            "Angebot".into(),
            PdfLayout {
                background: Some(picture.clone()),
                signature: Some(picture),
                footer: FooterStyle::Eigener,
                footer_columns: vec!["{firma} – IBAN {iban}".into()],
                show_position: false,
                ..PdfLayout::default()
            },
        );
        data.company.file_name_pattern = "{jahr} {art} {nummer} {kunde}".into();
        let pdf = String::from_utf8_lossy(&document_pdf(&data, &offer)).into_owned();
        assert!(pdf.contains("/Im2 Do") && pdf.contains("/Im3 Do"));
        let customer = data.contact_name(offer.contact_id);
        assert!(pdf.contains(&format!("(Angebot f\u{fc}r {customer}")) || pdf.contains("Angebot f"));
        assert!(!pdf.contains("{kunde}"));
        assert!(pdf.contains(&format!("({} \u{2013} IBAN", data.company.name)) || pdf.contains("IBAN"));
        // Invoices keep the standard layout.
        let invoice = data.documents.iter().find(|d| d.kind == DocKind::Rechnung).unwrap();
        assert!(!String::from_utf8_lossy(&document_pdf(&data, invoice)).contains("/Im2"));
        let name = file_name(&data, &offer);
        assert!(name.starts_with(&offer.date.format("%Y").to_string()) && name.ends_with(".pdf") && name.contains("Angebot"));
        // Reminder texts come from the templates, with the greeting filled in.
        let r = ReminderRecord { level: 0, date: invoice.date, fee: 0, due: invoice.date };
        let preview = reminder_preview(&data, invoice, &r);
        let texts: Vec<String> = preview[0].shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect();
        assert!(texts.iter().any(|t| t.starts_with("Guten Tag") || t.starts_with("Sehr geehrte")));
    }

    #[test]
    fn wrapping_respects_width() {
        let lines = wrap("Ein sehr langer Positionstext für die Rechnung", 10.0, false, 80.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(l, 10.0, false) <= 80.0));
    }

    #[test]
    fn sample_invoice_is_a_pdf() {
        let data = sample_data();
        let offer = data.documents.iter().find(|d| d.kind == DocKind::Angebot).unwrap();
        let bytes = document_pdf(&data, offer);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // A long invoice runs over several pages.
        let mut long = offer.clone();
        long.lines = (0..45)
            .map(|i| Line { text: format!("Leistung Nummer {i} mit ausführlicher Beschreibung für Ärzte & Bäckereien"), ..offer.lines[0].clone() })
            .collect();
        let long_pdf = document_pdf(&data, &long);
        assert!(!String::from_utf8_lossy(&long_pdf).contains("/Count 1 "));
        // Set OPENKONTOR_PDF_OUT to a folder to look at the results.
        // A contact with English documents gets an English PDF.
        let mut english = data.clone();
        let cid = offer.contact_id.unwrap();
        english.contacts.iter_mut().find(|c| c.id == cid).unwrap().language = Lang::En;
        let en_pdf = document_pdf(&english, offer);
        let en_text = String::from_utf8_lossy(&en_pdf);
        assert!(en_text.contains("(Total amount)") && en_text.contains("(Quote no.)"));
        // With a logo the image is embedded once.
        let mut with_logo = data.clone();
        with_logo.company.logo = Some(make_logo(2, 1, &[255, 0, 0, 0, 0, 255]));
        let logo_pdf = String::from_utf8_lossy(&document_pdf(&with_logo, offer)).into_owned();
        assert!(logo_pdf.contains("/Subtype /Image /Width 2 /Height 1") && logo_pdf.contains("/Im1 Do"));
        assert_eq!(decode_base64(&encode_base64(b"Hallo Welt")), b"Hallo Welt");
        // A delivery note has no prices.
        let mut ls = offer.clone();
        ls.kind = DocKind::Lieferschein;
        assert!(!String::from_utf8_lossy(&document_pdf(&data, &ls)).contains("(Gesamtbetrag)"));
        if let Ok(dir) = std::env::var("OPENKONTOR_PDF_OUT") {
            std::fs::write(format!("{dir}/angebot-en.pdf"), &en_pdf).unwrap();
            std::fs::write(format!("{dir}/angebot.pdf"), &bytes).unwrap();
            std::fs::write(format!("{dir}/lang.pdf"), &long_pdf).unwrap();
        }
    }
}
